
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use seed::{*, prelude::*};
use serde_json::Value as JsonValue;
use syn::{LitInt, LitStr, LitBool, ExprArray, Expr, ExprLit, Lit};
use proc_macro2::Span;

use ellocopo2::owned::{Msg as ProtoMsg, Value};
use ellocopo2::TypeTag;
use ellocopo2::RequestCode;
use ellocopo2::AnswerCode;

mod parse;

const ENTER_KEY: u32 = 13;
const ESC_KEY: u32 = 27;

#[derive(Default)]
pub struct Model {
    // Actual static scheme tree
    trees: Vec<Tree>,
    // Current values associated with certain leaf
    leafs: HashMap<String, Rc<RefCell<TLeaf>>>,
}


#[derive(Clone, Debug)]
pub enum Msg {
    SetScheme(String),
    SumbmitRequest(String, RequestCode),
    GRequestUpdate(ProtoMsg),
    GAnswerUpdate(Result<ProtoMsg, String>),
    InputUpdated(String, String),
    FoldNode(Rc<RefCell<TNode>>),
}


pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::SetScheme(scheme) => {
            let scheme: JsonValue = serde_json::from_str(&scheme).unwrap();
            //log!(&scheme);
            let trees = parse::tree_model(scheme).unwrap();
            //log!(&trees);
            let leafs = parse::build_view_leaf(&trees);
            //log!(&leafs);

            model.trees = trees;
            model.leafs = leafs;
        }
        Msg::SumbmitRequest(path, op) => {
            let TLeaf{ty, view: ViewLeaf{input_val, ..}, ..} = &*model.leafs[&path].borrow();
            
            let val = if let RequestCode::WRITE = op { 
                match decorator_string_to_value(input_val, *ty) {
                    Ok(val) => {
                        log::info!("Parsed val: {:?}", &val);
                        val
                    }
                    Err(err) => {
                        return crate::alert(&format!("Failed to parse: {:?}", err));
                    }
                }
            } else { Value::UNIT(()) };
            
            let msg = ProtoMsg(op.into(), path, val);
            orders.send_msg(Msg::GRequestUpdate(msg));
        }
        Msg::GAnswerUpdate(ans_res) => {
            
            match ans_res {
                Ok(ProtoMsg(AnswerCode::OK_READ, path, inval)) => {
                    let TLeaf{view: ViewLeaf{val, ..}, ..} = &mut *model.leafs[&path].borrow_mut();
                    *val = Some(inval);
                }
                Ok(ProtoMsg(AnswerCode::OK_WRITE, path, val)) => {
                    log::info!("OK_WRITE {} {:?}", path, val)
                }
                Err(err) => {
                    crate::alert(&format!("Answer update error: {}", err));
                }
                _ => (),
            }
        }
        Msg::InputUpdated(path, input) => {
            log!(path, input);
            let TLeaf{view: ViewLeaf{input_val, ..}, ..} = &mut *model.leafs[&path].borrow_mut();
            *input_val = input;
        }
        Msg::FoldNode(node) => {
            let fold = &mut node.borrow_mut().view.fold;
            *fold = !*fold;
        }
        msg @ _ => log!(msg),
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    ul![ 
        span!["Список комманд:"],
        {
            let mut content = Vec::new();
            for tree in &model.trees {
                content.push(view_tree(tree));
            }
            content
        }
    ]
}

fn view_tree(tree: &Tree) -> Node<Msg> {
    match tree {
        Tree::TNodeV(node) => {
            let mut content = Vec::new();
            for i in &node.borrow().children {
                content.push(view_tree(i))
            }
            li![
                span![
                    &node.borrow().name,
                    simple_ev(Ev::Click, Msg::FoldNode(Rc::clone(node))),
                ],
                if !node.borrow().view.fold {
                    ul![content]
                } else { empty![] }
            ]
        }
        Tree::TLeafV(leaf) => {
            view_leaf(&leaf.borrow())
        }
    }
}

fn view_leaf(TLeaf{name, path, meta: MetaDesc{w, r, ..}, view: ViewLeaf {input_val, val, ..}, ..}: &TLeaf) -> Node<Msg> {
    
    li![
        id![&path],
        span![format!("{}:", name)],
        if *r {
            vec![
                span![format!("{:?}", val)],
                button![
                    C!["view-rbutton"],
                    "(R)",
                    {
                        let path = path.clone();
                        input_ev(Ev::Click, move |_| Msg::SumbmitRequest(path, RequestCode::READ))
                    },
                ],
            ]
        } else { vec![empty![]] },
        if *w {
            vec![
                input![
                    C!["view-input"],
                    attrs! {
                        At::Placeholder => "Input data",
                        At::Value => input_val,
                    },
                    {
                        let path = path.clone();
                        keyboard_ev(Ev::KeyDown, move |keyboard_event| {
                            IF!(keyboard_event.key_code() == ENTER_KEY => Msg::SumbmitRequest(path, RequestCode::WRITE))
                        })
                    },
                    {
                        let path = path.clone();
                        input_ev(Ev::Input, move |txt| Msg::InputUpdated(path, txt))
                    }
                ],
                button![
                    C!["view-wbutton"],
                    "(W)",
                    {
                        let path = path.clone();
                        input_ev(Ev::Click, move |_| Msg::SumbmitRequest(path, RequestCode::WRITE))
                    },
                ],
            ]
        } else { vec![empty![]] },
    ]
}

#[derive(Clone, Debug)]
pub enum Tree {
    TNodeV(Rc<RefCell<TNode>>),
    TLeafV(Rc<RefCell<TLeaf>>),
}

impl Default for Tree {
    fn default() -> Self {
        Tree::TNodeV(
            Rc::new(RefCell::new(
                TNode {
                    name: "".to_string(),
                    path: "".to_string(),
                    meta: MetaDesc {
                        w: false,
                        r: false,
                    },
                    view: Default::default(),
                    children: vec![],
                }
            ))
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct ViewLeaf {
    input_val: String,
    input_def: Option<String>,
    val: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ViewNode {
    fold: bool,
}

impl Default for ViewNode {
    fn default() -> Self {
        Self {
            fold: true,
        }
    }
}


#[derive(Clone, Debug)]
pub struct TNode {
    name: String,
    path: String,
    meta: MetaDesc,
    view: ViewNode,
    children: Vec<Tree>,
}

#[derive(Clone, Debug)]
pub struct TLeaf {
    name: String,
    path: String,
    meta: MetaDesc,
    ty: TypeTag,
    view: ViewLeaf,
}

#[derive(Clone, Copy, Debug)]
struct MetaDesc {
    w: bool, // Write rights
    r: bool, // Read rights
}

impl Default for MetaDesc {
    fn default() -> Self {
        Self {
            w: false,
            r: true,
        }
    }
}

fn decorator_string_to_value(i: &str, hint: TypeTag) -> Result<Value, syn::Error>  {
    match hint {
        TypeTag::STR => {
            let wrapped_i = "\"".to_string() + i + "\"";
            string_to_value(&wrapped_i)
        }
        TypeTag::BYTES => {
            let wrapped_i = "[".to_string() + i + "]";
            string_to_value(&wrapped_i)
        }
        _ => string_to_value(i),
    }
}

// TODO: move to ellocopo2::parse under std feat
fn string_to_value(i: &str) -> Result<Value, syn::Error>  {
    if i.is_empty() || i == "()"{
        return Ok(Value::UNIT(()));
    }
    
    let val = if let Ok(res) = syn::parse_str::<LitInt>(i) {
        match res.suffix() {
            "u8"  => { Value::U8(res.base10_parse::<u8>()?) },
            "u16" => { Value::U16(res.base10_parse::<u16>()?) },
            "u32" => { Value::U32(res.base10_parse::<u32>()?) },
            "i32" | "" => { Value::I32(res.base10_parse::<i32>()?) },
            _ => return Err(syn::Error::new(Span::call_site(), 
                    "Failed to parse Int, not supported literal prefix")),
        }
    } 
    else if let Ok(res) = syn::parse_str::<LitBool>(i) {
        Value::BOOL(res.value)
    } 
    else if let Ok(res) = syn::parse_str::<LitStr>(i) {
        Value::STR(res.value())
    }
    else if let Ok(res) = syn::parse_str::<ExprArray>(i) {
        let mut bytes = Vec::new();
        for e in res.elems {
            log!(&e);
            if let Expr::Lit(ExprLit {lit: Lit::Int(elem), ..}) = e {
                let elem = elem.base10_parse::<u8>()?;
                bytes.push(elem);
            } else {
                return Err(syn::Error::new(Span::call_site(), "Failed to parse array element"));
            }
        }
        Value::BYTES(bytes)
    }
    else {
        return Err(syn::Error::new(Span::call_site(), "else clause"));
    };

    Ok(val)
}



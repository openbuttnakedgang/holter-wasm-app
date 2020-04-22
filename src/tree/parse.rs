
use serde_json::map::Map;

use super::*;

const ANNOTATION_TOKEN : &'static str = "@";
const ANNOTATION_ACCESS_STR : &'static str = "@access";
const ANNOTATION_TYPE_STR   : &'static str = "@type";
const REGISTER_PATH_DELIMETR: &'static str = "/";

pub fn tree_model(root: JsonValue) -> Result<Vec<Tree>, String>{

    // Default meta RO
    let meta = MetaDesc::default();
    // Prefix path with root elem /
    let mut children = Vec::new();

    // root object
    if let JsonValue::Object(root) = root {
        for (name, fields) in root {
            if filter_nodes(&name) {
                let new_path = "/".to_string() + &name;
                children.push(visit_tree(&new_path, &name, &fields, meta)?);
            }
        }
    } else {
        Err("Non root object".to_string())?
    };

    Ok(children)
}

fn filter_nodes(name: &String) -> bool {
    // filter @annotations
    !name.starts_with(ANNOTATION_TOKEN)
}

fn visit_tree(path: &String, name: &String, value: &JsonValue, meta: MetaDesc) -> Result<Tree, String> {
    Ok(match value {
        JsonValue::Object(fields) => visit_node(path, name, fields, meta)?,
        JsonValue::String(ty_s) => { 
            let ty = ty_convert(ty_s)?;
            visit_leaf(path, name, ty, meta)?
        }
        err_str @ _ => Err(&format!("Unexpected entity in parse tree: {:?}", err_str))?,
    })
}

fn visit_node(path: &String, name: &String, fields: &Map<String, JsonValue>, meta: MetaDesc) -> Result<Tree, String> {
    let meta = extract_meta(fields, meta);
    
    // Test for nested register definition
    let res = match extract_ty(fields) {
        // It's nested register definition, proceed to creating a leaf
        Some(ty) => {
            visit_leaf(path, name, ty, meta)?
        }
        // None => then it's nested section, so continue recursively
        None => {
            let mut children = Vec::new();
            for (name, keys) in fields {
                if filter_nodes(&name) {
                    let new_path = path.clone() + REGISTER_PATH_DELIMETR + name;
                    children.push(
                        visit_tree(&new_path, name, keys, meta)?
                    );
                }
            }

            Tree::TNodeV(Rc::new(RefCell::new(TNode {
                name: name.clone(),
                path: path.clone(),
                meta,
                view: Default::default(),
                children,
            })))
        }
    };

    Ok(res)
}

fn visit_leaf(path: &String, name: &String, ty: TypeTag, meta: MetaDesc) -> Result<Tree, String> {

    // WO behaviour for UNIT ty
    let meta = if let TypeTag::UNIT = ty {
        MetaDesc{w: true, r: false, .. meta}
    } else { meta };

    Ok(Tree::TLeafV(Rc::new(RefCell::new(TLeaf {
        name: name.clone(),
        path: path.clone(),
        meta,
        ty,
        view: Default::default(),
    }))))
}

fn extract_ty(fields: &Map<String, JsonValue>) -> Option<TypeTag> {
    let mut ty = None;
    for (k,v) in fields {
        if k.starts_with(ANNOTATION_TYPE_STR) {
            if let JsonValue::String(tyy) = v {
                ty = Some(ty_convert(tyy).unwrap());
            } else  {
                panic!("Wrong type in @type")
            }
        }
    }
    ty
}

fn extract_meta(fields: &Map<String, JsonValue>, inhereted_meta: MetaDesc) -> MetaDesc {
    let mut meta = inhereted_meta;
    for (k,v) in fields {
        if k.starts_with(ANNOTATION_ACCESS_STR) {
            if let JsonValue::String(rights) = v {
                let Access{ w, r} = access_convert(rights)
                    .expect("Malformed access rights format");
                meta.w = w;
                meta.r = r;
            } else  {
                panic!("Malformed access rights inner type")
            }
        }
    }
    meta
}

fn ty_convert(tytag: &String) -> Result<TypeTag, String> {
    let ty = match tytag.as_str() {
        "()"   => TypeTag::UNIT,
        "bool" => TypeTag::BOOL,
        "u8"   => TypeTag::U8,
        "i32"  => TypeTag::I32,
        "u32"  => TypeTag::U32,
        "str"  => TypeTag::STR,
        "[u8]" => TypeTag::BYTES,
        _      => return Err(format!("Unsupproted type: {}", &tytag)),
    };
    Ok(ty)
}

struct Access {
    w: bool,
    r: bool,
}

fn access_convert(access: &String) -> Result<Access, String> {
    let access = match access.as_str() {
        "WO" => Access{ w: true, r: false },
        "RO" => Access{ w: false, r: true },
        "RW" => Access{ w: true, r: true },
        _    => return Err(format!("Unsupproted access meta: {}", &access)),
    };
    Ok(access)
}

pub fn build_view_leaf(trees: &Vec<Tree>) -> HashMap<String, Rc<RefCell<TLeaf>>> {
    fn inner_visit(tree: &Tree) -> HashMap<String, Rc<RefCell<TLeaf>>> {
        let mut map = HashMap::new();
        match tree {
            Tree::TNodeV(node) => {
                for n in &node.borrow().children{
                    map.extend(inner_visit(&n))
                }
                map
            }
            Tree::TLeafV(leaf) => {
                map.insert(leaf.borrow().path.clone(), Rc::clone(leaf));
                map
            }
        }
    }
    
    let mut map = HashMap::new();
    for tr in trees {
        map.extend(inner_visit(tr));
    }

    map
}

fn defaults_inputs_view_leaf(_leafs: &mut HashMap<String, TLeaf>) {
    let _defs: HashMap<String, String> = HashMap::new();
    // TODO:
    todo!()
}















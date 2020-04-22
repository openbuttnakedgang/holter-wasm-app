use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use gloo_timers::future::TimeoutFuture;
use seed::{*, prelude::*};

mod tree;

#[derive(Default)]
struct Model {
    pub teee: tree::Model,
    pub device: Option<Rc<HolterDevice>>,
    pub descriptor: Option<String>,
}

#[derive(Clone)]
enum Msg {
    Tree(tree::Msg),
    ConnectButton,
    NewDevice(Rc<HolterDevice>),
    SchemeLoaded,
    SendCmd,
    RecCmd,
    Nothing,
}

fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::Tree(tree::Msg::GRequestUpdate(msg)) => {
            //crate::alert(&format!("Global: name {}, val {}", name, val));
            
            let device =  Rc::clone(model.device.as_ref().unwrap());
            orders.perform_cmd(
                handle_cmd(device, msg)
            );
        }
        Msg::Tree(msg) => { 
            tree::update(msg, &mut model.teee, &mut orders.proxy(Msg::Tree));
        }
        Msg::ConnectButton => {
            log!("ConnectButton pressed");
            orders
                .perform_cmd(request_device());
        }
        Msg::NewDevice(dev) => {
            model.device = Some(dev);
            let desc: HashMap<String,String> = model.device
                .as_ref()
                .unwrap()
                .descriptor()
                .into_serde()
                .unwrap();
            let desc_s: String = desc
                .iter()
                .fold("".into(), |acc, (key, value)| acc + key + ": " + value + ", ");
            model.descriptor = Some(desc_s);
        }
        msg @ _ => log!(msg),
    }
}

// ------ ------
// Orders
// ------ ------

//async fn inc_after_delay() -> Msg {
//    TimeoutFuture::new(1_000).await;
//    Msg::Increment
//}

async fn request_device() -> Msg {
    let result = wasm_bindgen_futures::JsFuture::from(requestDevice()).await;
    let val = result.unwrap();
    log!(&val);
    let dev: HolterDevice = JsCast::dyn_into(val).unwrap();
    // connect
    let result = wasm_bindgen_futures::JsFuture::from(dev.connect()).await;
    let val = result.unwrap();
    log!("connect ", val);


    //let v = js_sys::Uint8Array::new(&val).to_vec();

    Msg::NewDevice(Rc::new(dev))
}


use std::convert::TryInto;
use ellocopo2::RequestBuilder;
//use ellocopo2::Value as NotOwnValue;
//use ellocopo2::owned::Value as Value;
//use ellocopo2::Msg as NotOwnMsg;
use ellocopo2::owned::Msg as ProtoMsg;
//use ellocopo2::owned::Value;
use ellocopo2::RequestCode;
use ellocopo2::AnswerCode;
use ellocopo2::ParseMsg;
use ellocopo2::ParserError;
use ellocopo2::MAX_MSG_SZ;

async fn handle_cmd(device: Rc<HolterDevice>, ProtoMsg(code, ref path, ref value): ProtoMsg) -> Msg {
    
    let mut buf_out = [0u8;MAX_MSG_SZ];
    let buf_out = {
        let mut req = RequestBuilder::new(&mut buf_out);
        let sz = req 
            .path(&path)
            .code(code.try_into().unwrap())
            .payload(value.into())
            .build()
            .unwrap();
        &buf_out[..sz]
    };

    // Allocate recv transaction
    let future_in = wasm_bindgen_futures::JsFuture::from(device.recv_cmd());
    
    // Send
    let result_out = wasm_bindgen_futures::JsFuture::from(device.send_cmd(buf_out)).await;
    let val_out = result_out.unwrap();
    js_debug(&val_out);
    
    // Awaiting recv callback
    let result_in = future_in.await;
    let val_in = result_in.unwrap();
    js_debug(&val_in);
    let data_view = js_sys::Reflect::get(&val_in, &JsValue::from_str("data")).unwrap();
    js_debug(&data_view);
    let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
    js_debug(&array_buf);

    let mut cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
    log::info!("{:?}",cmd_buf);
    
    let mut parser = ParseMsg::new();
    let mut parsed_msg: Option<ProtoMsg> = None;
    while { let len = cmd_buf.len(); len < MAX_MSG_SZ } {

        let res = parser.try_parse(&cmd_buf);
        match res {
            Ok(msg) => {
                parsed_msg = Some(msg.into());
                break;
            }
            Err(ParserError::NeedMoreData) => {
                // Allocate recv transaction
                let future_in = wasm_bindgen_futures::JsFuture::from(device.recv_cmd());
                // Awaiting recv callback
                let result_in = future_in.await;
                let val_in = result_in.unwrap();
                js_debug(&val_in);
                let data_view = js_sys::Reflect::get(&val_in, &JsValue::from_str("data")).unwrap();
                js_debug(&data_view);
                let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
                js_debug(&array_buf);
                let tmp_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
                log::info!("{:?}", &tmp_buf);

                cmd_buf.extend(&tmp_buf);
            }
            Err(e) => panic!("{:?}", e),
        }
    }
    
    log::info!("{:#?}", parsed_msg.as_ref().unwrap());

    Msg::Tree(tree::Msg::GAnswerUpdate(Ok(parsed_msg.unwrap())))
}

async fn fetch_scheme() -> Msg {
    let response = fetch("public/scheme.json").await.expect("HTTP request failed");

    let user: String = response
        .check_status() // ensure we've got 2xx status
        .expect("status check failed")
        .text()
        .await
        .expect("Failed to des");

    Msg::Tree(tree::Msg::SetScheme(user))
}

// ------ ------
// View
// ------ ------

fn view(model: &Model) -> Vec<Node<Msg>> {
    vec![
        div![
            C!["row"],
            button![
                C!["two columns"],
                simple_ev(Ev::Click, Msg::ConnectButton),
                "Connect",
                if let Some(_) = &model.device {
                    attrs!{
                        At::Disabled => true
                    }
                } else {
                    attrs!{}
                }
            ],
            progress![
                C!["ten columns"],
                attrs!{
                    At::Max => 100,
                    At::Value => 50,
                }
            ]
        ],
        div![
            C!["container"],
            if let Some(desc) = &model.descriptor {
                &desc
            } else {
                "No connected devices!"
            }
        ],
        div![
            C!["container"],
            tree::view(&model.teee).map_msg(Msg::Tree)
        ]
    ]
}

// ------ ------
// Window Events
// ------ ------

//fn window_events(_model: &Model) -> Vec<EventHandler<Msg>> {
//    vec![
//        simple_ev(Ev::Load, Msg::Init),
//    ]
//}

fn after_mount(_url: Url, orders: &mut impl Orders<Msg>) -> AfterMount<Model> {
    orders
        .perform_cmd(fetch_scheme());
    AfterMount::default()
}

// ------ ------
// Bindings
// ------ ------

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
    fn js_debug(v: &JsValue);
}

#[wasm_bindgen]
extern "C" {

    type HolterDevice;

    #[wasm_bindgen(method)]
    fn connect(this: &HolterDevice) -> js_sys::Promise;

    #[wasm_bindgen(method)]
    fn send_cmd(this: &HolterDevice, data: &[u8]) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn recv_cmd(this: &HolterDevice) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn descriptor(this: &HolterDevice) -> js_sys::Map;

    #[wasm_bindgen(method, getter)]
    fn number(this: &HolterDevice) -> u32;
    #[wasm_bindgen(method, setter)]
    fn set_number(this: &HolterDevice, number: u32) -> HolterDevice;

    #[wasm_bindgen]
    pub fn f1() -> js_sys::Promise;

    #[wasm_bindgen]
    pub fn requestDevice() -> js_sys::Promise;

}

#[wasm_bindgen(start)]
pub fn render() {

    wasm_logger::init(wasm_logger::Config::default());

    let window = web_sys::window().expect("no global `window` exists");
    log!("Widnow!: {:?}", &window);

    App::builder(update, view)
        //.window_events(window_events)
        .after_mount(after_mount)
        .build_and_start();
}

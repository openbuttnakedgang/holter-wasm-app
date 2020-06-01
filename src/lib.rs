
use std::rc::Rc;
use std::cell::RefCell;

//#[macro_use]
//extern crate log;

use wasm_bindgen::prelude::*;
//use wasm_bindgen::JsCast;
use gloo_timers::future::TimeoutFuture;
use seed::{*, prelude::*};

mod device;
mod cfg;
mod tree;

#[derive(Default)]
struct Model {
    treee: tree::Model,
    device: Rc::<device::Device>,
}

#[derive(Clone)]
enum Msg {
    Tree(tree::Msg),
    Connect,
    AutoConnect,
    DevConnected(Rc<device::Device>),
    //NewDevice(Rc<HolterDevice>),
    CfgLoaded(String),
    DownloadFile,
}

// dev: wasm-pack build --target web --out-name package --dev
// run server: cargo make serve


fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::Tree(tree::Msg::GRequestUpdate(msg)) => {
            let device =  Rc::clone(&model.device);
            orders.perform_cmd( async move {
                log::info!("Performing cmd");
                let dev_ans = device.send_recv_cmd(msg.clone()).await;
                match dev_ans {
                    Ok(msg) => { 
                        log::info!("End::Performing cmd");
                        Some(Msg::Tree(tree::Msg::GAnswerUpdate(Ok(msg))))
                    },
                    Err(e) => {
                        log::error!("{:?}", e);
                        log::info!("End::Performing cmd");
                        Some(Msg::Connect)
                    }
                }
            });
        }
        Msg::Tree(msg) => { 
            tree::update(msg, &mut model.treee, &mut orders.proxy(Msg::Tree));
        }
        Msg::Connect => {
            log!("Connect pressed");
            orders
                .perform_cmd(async {
                    let r = device::Device::request_device()
                        .await;
                    use device::Error;
                    match r {
                        Ok(dev) => Some(Msg::DevConnected(Rc::new(dev))),

                        Err(e @ Error::NotSelected) => { 
                            log::info!("{:?}", e);
                            None
                        }
                        Err(e @ Error::Security) => { 
                            log::error!("{:?}", e);
                            None
                        }
                        Err(e) => {
                            log::error!("{:?}", e);
                            Some(Msg::AutoConnect)
                        }
                    }
                });
        }
        Msg::AutoConnect => {
            orders.perform_cmd(async {auto_connect().await});
        }
        Msg::DevConnected(dev) => {

            if model.device.is_reconnecting(&dev) {
                log::info!("Device reconnected");
            } else {
                log::info!("New device connected");
                let desc = dev.descriptor().unwrap().clone();
                orders.perform_cmd(async {
                    let scheme = cfg::load(desc).await;
                    Msg::CfgLoaded(scheme)
                });
            };
            model.device = dev;
        }
        Msg::CfgLoaded(scheme) => {
            orders.send_msg(Msg::Tree(tree::Msg::SetScheme(scheme)));
        }
        Msg::DownloadFile => {
            let device = Rc::clone(&model.device);
            orders.perform_cmd(download_file(device));
        }
    }
}

// ------ ------
// Orders
// ------ ------

//async fn inc_after_delay() -> Msg {
//    TimeoutFuture::new(1_000).await;
//    Msg::Increment
//}

async fn auto_connect() -> Msg {
    TimeoutFuture::new(100).await;
    Msg::Connect
}

async fn download_file(device: Rc<device::Device>) -> Option<Msg> {
    use ellocopo2::owned::{Msg as DevMsg, Value};
    use ellocopo2::AnswerCode;

    async fn cmd(device: &Rc<device::Device>, msg: DevMsg) -> Result<Value, ()> {
        let dev_ans = device.send_recv_cmd(msg).await;
        match dev_ans {
            Ok(msg) => { 
                Ok(msg.2)
            },
            Err(e) => {
                log::error!("{:?}", e);
                return Err(());
            }
        }
    }
    
    let block_cnt = 0x10;

    log::info!("Performing download");

    let _ = cmd(&device, DevMsg (AnswerCode::OK_WRITE, String::from("/io/file/pos"), Value::U32(0)))
        .await
        .unwrap();

    let _ = cmd(&device, DevMsg (AnswerCode::OK_WRITE, String::from("/io/file/len"), Value::U32(block_cnt)))
        .await
        .unwrap();
    
    let _ = cmd(&device, DevMsg (AnswerCode::OK_WRITE, String::from("/io/file/start"), Value::UNIT(())))
        .await
        .unwrap();
    
    for _ in 0 .. block_cnt {
        let buf = device.recv_file_block().await.unwrap();
        log::info!("recv_file_block res {:?}", buf.len());
        log::info!("{:x?}", &buf[.. delta::defs::HEADER_SZ]);
        let mut parser = delta::block::parse::BlockParser::new();
        let r = parser.try_open_block(&buf[..]);
        log::info!("try_open_block res: {:?}", r);
        log::info!("Blk header: {:#?}", parser.header());
    }

    log::info!("End::Performing download");

    None
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
                simple_ev(Ev::Click, Msg::Connect),
                "Connect",
                if model.device.is_connected() {
                    attrs!{
                        At::Disabled => true
                    }
                } else {
                    attrs!{}
                }
            ],
        ],
        div![
            C!["container"],
            if let Some(desc) = model.device.descriptor() {
                format!("{}", desc)
            } else {
              "No connected devices!".into()
            }
        ],
        div![
            C!["container"],
            tree::view(&model.treee).map_msg(Msg::Tree)
        ],
        div![
            C!["container"],
            button![
                simple_ev(Ev::Click, Msg::DownloadFile),
                "Download file",
                if !model.device.is_connected() {
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

//fn after_mount(_url: Url, orders: &mut impl Orders<Msg>) -> AfterMount<Model> {
//    orders
//        .perform_cmd(async {});
//    AfterMount::default()
//}

// ------ ------
// Bindings
// ------ ------

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
    fn js_debug(v: &JsValue);
}

#[wasm_bindgen(start)]
pub fn render() {

    wasm_logger::init(wasm_logger::Config::default());

    let window = web_sys::window().expect("no global `window` exists");
    log!("Widnow!: {:?}", &window);

    App::builder(update, view)
        //.window_events(window_events)
//        .after_mount(after_mount)
        .build_and_start();
}




use std::rc::Rc;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

//#[macro_use]
//extern crate log;

use wasm_bindgen::prelude::*;
//use wasm_bindgen::JsCast;
use gloo_timers::future::TimeoutFuture;
use seed::{*, prelude::*};

mod device;
mod cfg;
mod tree;
mod vis;

#[derive(Default)]
struct Model {
    treee: tree::Model,
    device: Rc<device::Device>,
    vis: Rc<AtomicBool>,
    vis_group: Rc<Cell<VisSelectedGroup>>,
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
    VisStart,
    VisUpdate,
    VisSelectedGroup(VisSelectedGroup),
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
        Msg::VisStart => {
            // FIXME: proper vis stop sequence
            if model.vis.load(Ordering::SeqCst) {
                model.vis.store(false, Ordering::SeqCst);
                return;
            }

            let device = Rc::clone(&model.device);
            let vis = Rc::clone(&model.vis);
            orders.perform_cmd(vis_start(device, vis));
        }
        Msg::VisUpdate => {
            let device = Rc::clone(&model.device);
            let vis = Rc::clone(&model.vis);
            let vis_group = Rc::clone(&model.vis_group);
            orders.perform_cmd(vis_update(device, vis, vis_group));
        }
        Msg::VisSelectedGroup(group) => {
            log::info!("Selected vis group: {:?}", group);
            model.vis_group.set(group);
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

async fn download_file(device: Rc<device::Device>) -> Option<Msg> {

    log::info!("Performing download");

    let _ = cmd(&device, DevMsg (AnswerCode::OK_WRITE, String::from("/io/file/pos"), Value::U32(0)))
        .await
        .unwrap();

    let r = cmd(&device, DevMsg (AnswerCode::OK_READ, String::from("/io/file/len"), Value::UNIT(())))
        .await
        .unwrap();

    let block_cnt = if let Value::U32(_block_cnt) = r {
        0x10//block_cnt
    } else { unimplemented!() };

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

async fn vis_start(device: Rc<device::Device>, vis: Rc<AtomicBool>) -> Option<Msg> {
    log::info!("Vis started");
    let _ = cmd(&device, DevMsg (AnswerCode::OK_WRITE, String::from("/ctrl/vis"), Value::BOOL(true)))
        .await
        .unwrap();
    
    vis.store(true, Ordering::SeqCst);
    Some(Msg::VisUpdate)
}

async fn vis_update(
    device: Rc<device::Device>,
    vis: Rc<AtomicBool>,
    vis_group: Rc<Cell<VisSelectedGroup>>,
) 
    -> Option<Msg> 
{
    use std::sync::mpsc;
    use delta::block::parse::{PntResult, Point};
    use delta::point::decode::PointDesc;
    use delta::error::DecodingError;
    use delta::defs::GroupId;
    use VisSelectedGroup::*;
    
    let (tx, rx) = mpsc::channel();
    vis::vis_run(rx).unwrap();

    let mut buf = [0u8;0x800];
    let mut cnt = 0usize;
    loop {
        let sz = device.recv_vis(&mut buf).await.unwrap();
        if !vis.load(Ordering::SeqCst) {
            log::info!("Vis stopped");
            return None;
        }
        //log::info!("vis: {:x?}", &buf[.. sz]);
        let mut parser = delta::block::parse::BlockParser::new();
        let r = parser.try_open_block(&buf[.. sz]);
        //log::info!("try_open_block res: {:?}", r);
        //log::info!("Blk header: {:#?}", parser.header());
        if let DecodingError::Ok = r {
            while let PntResult::Ok(p) = parser.iter_point() {
                match p {
                    Point::PointV(PointDesc{group_id: GroupId::ECG, ch_cnt: 8}, sample) if vis_group.get() == ECG => {
                        let sample = Vec::from(sample)
                            .into_iter()
                            .map(|e| e / 100)
                            .collect();
                        if cnt % 8 == 0 {
                            tx.send(sample).unwrap();
                        }
                        cnt = cnt.wrapping_add(1);
                    }
                    Point::PointV(PointDesc{group_id: GroupId::REO, ch_cnt: 1}, sample) if vis_group.get() == REO => {
                        let sample = Vec::from(sample)
                            .into_iter()
                            .map(|e| e / 50)
                            .collect();
                            tx.send(sample).unwrap();
                    }
                    Point::PointV(PointDesc{group_id: GroupId::ACC_IN, ch_cnt: 3}, sample) if vis_group.get() == ACC_IN => {
                        let sample = Vec::from(sample)
                            .into_iter()
                            .map(|e| e / 8)
                            .collect();
                        tx.send(sample).unwrap();
                    }
                    Point::PointV(_, _) => (),
                    Point::EventV(buf) => {
                        log::error!("EVENT: {:x?}", buf);
                    }
                }
            }

        } else {
            log::error!("Failed to parse blk: {:?}", r);
        }
    }
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
        ],
        div![
            C!["container"],
            button![
                simple_ev(Ev::Click, Msg::VisStart),
                "Vis",
                if !model.device.is_connected() {
                    attrs!{
                        At::Disabled => true
                    }
                } else {
                    attrs!{}
                }
            ],
            select![
                input_ev(Ev::Change, |v| Msg::VisSelectedGroup(v.into())),
                option![ "ECG" ],
                option![ "REO" ],
                option![ "ACC_IN" ],
            ]
        ],
        div![
            canvas![
                id!("canvas"),
            ]
        ]
    ]
}


#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisSelectedGroup {
    ECG,
    REO,
    ACC_IN,
}

impl Default for VisSelectedGroup {
    fn default() -> Self {
        Self::ECG
    }
}

impl From<String> for VisSelectedGroup {
    fn from(v: String) -> Self {
        match v.as_str() {
            "ECG" => VisSelectedGroup::ECG,
            "REO" => VisSelectedGroup::REO,
            "ACC_IN" => VisSelectedGroup::ACC_IN,
            _ => unimplemented!(),
        }
    }
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



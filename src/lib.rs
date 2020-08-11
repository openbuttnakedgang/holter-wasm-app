
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
mod download;

#[derive(Default)]
struct Model {
    treee: tree::Model,
    device: Rc<device::Device>,
    vis: Rc<AtomicBool>,
    vis_group: Rc<Cell<VisSelectedGroup>>,
    upload_data: Option<Vec<u8>>,
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
    DfuUploadFirmware(web_sys::Event),
    DfuDownloadFirmware,
    UploadFileCompleted(Vec<u8>),
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
            orders.perform_cmd( async move {
                let cancel = false;
                download::download_file_from_device(
                    device,
                    "data.bin",
                    &cancel,
                ).await.unwrap();

                Option::<Msg>::None
            });
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
        Msg::DfuUploadFirmware(e) => {
            let event = e.dyn_into::<JsValue>().unwrap();
            let target  = js_sys::Reflect::get(&event, &JsValue::from_str("target")).unwrap();
            let files = js_sys::Reflect::get(&target, &JsValue::from_str("files")).unwrap();
            let files: web_sys::FileList = files.dyn_into().unwrap();
            let file = files.item(0).expect_throw("No file selected");
            log::info!("Upload file name: {}", file.name());

            orders.perform_cmd(upload_file(file));
        }
        Msg::UploadFileCompleted(data) => {
            // model.upload_data = Some(data);
            // log::info!("Upload file done, size: 0x{:x} bytes", model.upload_data.as_ref().unwrap().len());
            let device =  Rc::clone(&model.device);
            orders.perform_cmd( async move {
            let _dev_ans = device.send_recv_dfu(data).await;
            });
        }
        Msg::DfuDownloadFirmware => {
            let device =  Rc::clone(&model.device);
            orders.perform_cmd( async move {
            let dev_ans = device.dfu_upload().await;
            download::download_file("holter-firmware.bin".to_string(), dev_ans.unwrap()).await.unwrap();
            });
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

async fn upload_file(file: web_sys::File) -> Msg {
    let file: gloo_file::File = file.into();
    let bytes = gloo_file::futures::read_as_bytes(&file).await.unwrap();
    log::info!("Upload file data chunk:\n{:x?}", &bytes[.. if bytes.len() > 0x40 { 0x40 } else { bytes.len() }]);

    Msg::UploadFileCompleted(bytes)
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
            if !model.device.is_dfu_mode() {
                tree::view(&model.treee).map_msg(Msg::Tree)
            }
            else {
                div![]
            }
        ],
        div![
            C!["container"],
            button![
                simple_ev(Ev::Click, Msg::DownloadFile),
                "Download file",
                if model.device.is_dfu_mode() {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                } else {  
                    attrs!{};
                    style![]
                }
            ],
            progress![
                C!["ten columns"],
                if model.device.is_dfu_mode() {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                } else { 
                    attrs!{
                        At::Max => 100,
                        At::Value => 50,
                    };
                    style![]
                }
            ]
        ],
        div![
            button![
                "Upload file",
                ev(Ev::Click, |_| {
                    let elem: web_sys::HtmlElement = web_sys::window()
                        .unwrap()
                        .document()
                        .unwrap()
                        .get_element_by_id("upload-file")
                        .unwrap()
                        .dyn_into().unwrap();
                    elem.click();
                    ()
                }),
                if model.device.is_dfu_mode() {
                    attrs!{};
                    style![]
                } else {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                }
            ],
            input![
                id!["upload-file"],
                attrs![
                    At::Type => "file",
                ],
                style![
                    St::Display => "none",
                ],
                ev(Ev::Input, |e| Msg::DfuUploadFirmware(e)),
            ],
        ],
        div![
            C!["row"],
            button![
                C!["two columns"],
                simple_ev(Ev::Click, Msg::DfuDownloadFirmware),
                "Download",
                if model.device.is_dfu_mode() {
                    attrs!{};
                    style![]
                } else {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                }
            ],
        ],
        div![
            C!["container"],
            button![
                simple_ev(Ev::Click, Msg::VisStart),
                "Vis",
                if !model.device.is_connected() || model.device.is_dfu_mode() {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                } else {
                    attrs!{};
                    style![]
                }
            ],
            select![
                input_ev(Ev::Change, |v| Msg::VisSelectedGroup(v.into())),
                option![ "ECG" ],
                option![ "REO" ],
                option![ "ACC_IN" ],
                if !model.device.is_connected() || model.device.is_dfu_mode() {
                    attrs!{
                        At::Disabled => true
                    };
                    style![
                        St::Display => "none",
                        ]
                } else {
                    attrs!{};
                    style![]
                }
            ]
        ],
        div![
            canvas![
                id!("canvas"),
            ]
        ],
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



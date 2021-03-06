
use std::cell::RefCell;
use std::convert::TryInto;

use serde::Deserialize;
use seed::log;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::DomException;

use ellocopo2::RequestBuilder;
use ellocopo2::owned::Msg as DevMsg;
use ellocopo2::ParseMsg;
use ellocopo2::ParserError;
use ellocopo2::MAX_MSG_SZ;

use holter_dfu::dfu::defs::*;

use crate::js_debug;

#[wasm_bindgen]
extern "C" {
    type DeviceJs;

    #[wasm_bindgen(method)]
    fn js_connect(this: &DeviceJs) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_close(this: &DeviceJs) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_reset(this: &DeviceJs) -> js_sys::Promise;

    #[wasm_bindgen(method)]
    fn js_send_cmd(this: &DeviceJs, data: &[u8]) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_recv_cmd(this: &DeviceJs) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_recv_dfu(this: &DeviceJs, bRequest: u8, wLength: u16, wValue: u32) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_send_dfu(this: &DeviceJs, bRequest: u8, data: &[u8], wValue: u32) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_recv_file(this: &DeviceJs, size: u32) -> js_sys::Promise;
    #[wasm_bindgen(method)]
    fn js_recv_vis(this: &DeviceJs, size: usize) -> js_sys::Promise;

    #[wasm_bindgen(method)]
    fn js_descriptor(this: &DeviceJs) -> js_sys::Map;

    #[wasm_bindgen]
    pub fn js_requestDevice() -> js_sys::Promise;
}


#[derive(Debug)]
pub enum Error {
    NotConnected,
    NotSelected,
    Security,
    DomExp(DomException),
    RawJs(JsValue),
    DevTypeApi,
    EpStall,
}

#[derive(Debug, Clone, Copy)]
pub enum Type {
    Holter,
    Loader,
}

#[allow(non_snake_case)]
#[derive(Default, Debug)]
#[derive(Deserialize, Clone, Eq, PartialEq)]
pub struct Desc {
    productName: String,
    serialNumber: String,
    manufacturerName: String,
    pid: u16,
    vid: u16,
}

#[derive(Default)]
pub struct Device {
    ty: Type,
    desc: Desc,
    d: RefCell<Option<DeviceJs>>,
}

impl Device {
    pub async fn request_device() -> Result<Device, Error> {
        let dev = DeviceJs::request_device()
            .await?;
        
        let desc = dev.descriptor()
            .await?;

        dev.reset().await?;

        let ty = match desc.pid {
            0xBABA => Type::Holter,
            0xDEDA => Type::Loader,
            _ => unreachable!("Wrong pid"),
        };
        
        Ok(Device {
            ty,
            desc,
            d: RefCell::new(Some(dev)),
        })
    }

    pub fn descriptor(&self) -> Option<&Desc> {
        if self.d.borrow().is_some() {
            Some(&self.desc)
        } else {
            None
        }
    }

    pub fn _dev_type(&self) -> Option<Type> {
        if self.d.borrow().is_some() {
            Some(self.ty)
        } else {
            None
        }
    }

    pub fn is_connected(&self) -> bool {
        self.d.borrow().is_some()
    }

    pub fn is_dfu_mode(&self) -> bool {
        if let Type::Loader = self.ty { return true }
        false
    }

    pub fn is_reconnecting(&self, dev: &Self) -> bool {
        if self.desc == dev.desc {
            true
        } else {
            false
        }
    }

    pub async fn send_recv_cmd(&self, msg: DevMsg) -> Result<DevMsg,Error> {
        if let Type::Loader = self.ty { return Err(Error::DevTypeApi); }
        if !self.is_connected() { return Err(Error::NotConnected) }
        
        let r = {
            let dev = self.d.borrow();
            DeviceJs::send_recv_cmd(dev.as_ref().unwrap(), msg)
                .await
        };
        
        if r.is_err() {
            let mut dev = self.d.borrow_mut();
            *dev = None;
        }

       Ok(r?)
    }

    pub async fn send_recv_dfu(&self, data: Vec<u8>) -> Result<(), Error> {
        if let Type::Holter = self.ty { return Err(Error::DevTypeApi); }
        if !self.is_connected() { return Err(Error::NotConnected) }
        
        let r = {
            let dev = self.d.borrow();
            DeviceJs::send_recv_dfu(dev.as_ref().unwrap(), data)
                .await
        };
        
        if r.is_err() {
            let mut dev = self.d.borrow_mut();
            *dev = None;
        }

       Ok(r?)
    }

    pub async fn dfu_upload(&self) -> Result<Vec<u8>, Error> {
        if let Type::Holter = self.ty { return Err(Error::DevTypeApi); }
        if !self.is_connected() { return Err(Error::NotConnected) }
        
        let r = {
            let dev = self.d.borrow();
            DeviceJs::dfu_upload(dev.as_ref().unwrap())
                .await
        };
        
        if r.is_err() {
            let mut dev = self.d.borrow_mut();
            *dev = None;
        }

       Ok(r?)
    }
    
    pub async fn recv_file_block(&self, tran_size: u32) -> Result<Vec<u8>,Error> {
        if let Type::Loader = self.ty { return Err(Error::DevTypeApi); }
        if !self.is_connected() { return Err(Error::NotConnected)}

        let r = { 
            let dev = self.d.borrow();
            DeviceJs::recv_file(dev.as_ref().unwrap(), tran_size).await
        };

        if r.is_err() {
            let mut dev = self.d.borrow_mut();
            *dev = None;
        }

        Ok(r?)
    }

    pub async fn recv_vis(&self, buf: &mut [u8]) -> Result<usize,Error> {
        if let Type::Loader = self.ty { return Err(Error::DevTypeApi); }
        if !self.is_connected() { return Err(Error::NotConnected)}

        let r = { 
            let dev = self.d.borrow();
            DeviceJs::recv_vis(dev.as_ref().unwrap(), buf).await
        };

        if r.is_err() {
            let mut dev = self.d.borrow_mut();
            *dev = None;
        }

        Ok(r?)
    }
}

impl DeviceJs {

    async fn request_device() -> Result<DeviceJs,JsValue> {
        let result = wasm_bindgen_futures::JsFuture::from(js_requestDevice()).await;
        let val = result?;
        log!(&val);
        let dev: DeviceJs = JsCast::dyn_into(val)?;
        // connect
        let result = wasm_bindgen_futures::JsFuture::from(dev.js_connect()).await;
        let val = result?;
        log!("connect ", val);

        //let v = js_sys::Uint8Array::new(&val).to_vec();
        
        Ok(dev)
    }

    async fn descriptor(&self) -> Result<Desc,JsValue> {
        let desc: Desc = self.js_descriptor()
            .into_serde()
            .unwrap();

        log::info!("dev desc: {:#?}", &desc);
        Ok(desc)
    }

    async fn _close(&self) -> Result<(),JsValue> {
        let f = wasm_bindgen_futures::JsFuture::from(self.js_close());
        let _ = f.await?;
        Ok(())
    }

    async fn reset(&self) -> Result<(),JsValue> {
        let f = wasm_bindgen_futures::JsFuture::from(self.js_reset());
        let r = f.await?;
        log!("Device reset: ", r);
        Ok(())
    }

    async fn send_recv_dfu(&self, data: Vec<u8>) -> Result<(), Error> {
        
        log::info!("DFU");

        // Allocating recv transaction
        //let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_cmd());
        
        // Send
        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_GETSTATE, 1, 0));
        let msg_ans = future_in.await?;
        let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
        let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
        let cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
        log::info!("IN => {:x?}",cmd_buf);
        log::info!("data len{:?}", &data.len());

        let mut j: usize = 0;
        let mut i = 0;
        while j < data.len() {
            if data.len() > j + 64 {
                let future_in = wasm_bindgen_futures::JsFuture::from(self.js_send_dfu(dfu_request::DFU_DNLOAD, &data[j..j+64], i));
                let _msg_ans = future_in.await?;
            }
            else {
                let future_in = wasm_bindgen_futures::JsFuture::from(self.js_send_dfu(dfu_request::DFU_DNLOAD, &data[j..data.len()], i));
                let _msg_ans = future_in.await?;
            }

            let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_GETSTATUS, 6, 0));
            let _msg_ans = future_in.await?;
            j += 64;
            i += 1;
            log::info!("packet size: 64");
        }
        let buf = [0u8; 0];
        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_send_dfu(dfu_request::DFU_DNLOAD, &buf, i));
        let _msg_ans = future_in.await?;

        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_GETSTATUS, 6, 0));
        let msg_ans = future_in.await?;
        let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
        let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
        let cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
        log::info!("output {:?}",cmd_buf);

        Ok(())
    }

    async fn dfu_upload(&self) -> Result<Vec<u8>, Error> {
        log::info!("DFU");

        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_GETSTATUS, 6, 0));
        let msg_ans = future_in.await?;
        let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
        let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
        let cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
        log::info!("IN => {:x?}",cmd_buf);

        let mut state = 0;
        let mut data: Vec<u8> = Vec::new();

        while state != 2 {
            let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_UPLOAD, 64, 0));
            let msg_ans = future_in.await?;
            let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
            let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
            let mut cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
            log::info!("cmd buf len: {}", &cmd_buf.len());
            data.append(&mut cmd_buf);

            let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_dfu(dfu_request::DFU_GETSTATE, 1, 0));
            let msg_ans = future_in.await?;
            let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
            let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
            let cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
            state = cmd_buf[0];
        }
        log::info!("dta len: {}", &data.len());

        Ok(data)
    }

    async fn send_recv_cmd(&self, msg: DevMsg) -> Result<DevMsg, JsValue> {
        
        log::info!("OUT => {:#?}", &msg);
        let DevMsg(code, ref path, ref value) = msg;
        
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
        log::info!("OUT => {:x?}", &buf_out);

        // Allocating recv transaction
        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_cmd());
        
        // Send
        let _send_ok = wasm_bindgen_futures::JsFuture::from(self.js_send_cmd(buf_out))
            .await?;
        js_debug(&_send_ok);
        
        // Awaiting recv future
        let msg_ans = future_in.await?;
        js_debug(&msg_ans);
        let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
        js_debug(&data_view);
        let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
        js_debug(&array_buf);

        let mut cmd_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
        log::info!("IN => {:x?}",cmd_buf);
        
        let mut parser = ParseMsg::new();
        let mut parsed_msg: Option<DevMsg> = None;
        while { let len = cmd_buf.len(); len < MAX_MSG_SZ } {

            let res = parser.try_parse(&cmd_buf);
            match res {
                Ok(msg) => {
                    parsed_msg = Some(msg.into());
                    break;
                }
                Err(ParserError::NeedMoreData) => {
                    // Allocate recv transaction
                    let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_cmd());
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
        
        log::info!("IN => {:#?}", parsed_msg.as_ref().unwrap());

        Ok(
            parsed_msg.unwrap()
        )
    }

    async fn recv_file(&self, tran_size: u32) -> Result<Vec<u8>, Error> {
        
        // Allocating recv transaction
        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_file(tran_size));

        // Awaiting recv future
        let trans_result = future_in.await?;
        // Check transfer status
        let status = js_sys::Reflect::get(&trans_result, &JsValue::from_str("status")).unwrap();

        match status.as_string().unwrap().as_str() {
            "stall" => Err(Error::EpStall),
            "ok" => {
                //js_debug(&msg_ans);
                let data_view = js_sys::Reflect::get(&trans_result, &JsValue::from_str("data")).unwrap();
                //js_debug(&data_view);
                let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
                //js_debug(&array_buf);

                let buf = js_sys::Uint8Array::new(&array_buf).to_vec();
                //log::info!("{:x?}",buf);
    
                 Ok(buf)
            }
            s => panic!("File transaction unknow status: {}", s),
        }
    }

    async fn recv_vis(&self, buf: &mut [u8]) -> Result<usize, JsValue> {
        
        // Allocating recv transaction
        let future_in = wasm_bindgen_futures::JsFuture::from(self.js_recv_vis(buf.len()));

        // Awaiting recv future
        let msg_ans = future_in.await?;
        //js_debug(&msg_ans);
        let data_view = js_sys::Reflect::get(&msg_ans, &JsValue::from_str("data")).unwrap();
        //js_debug(&data_view);
        let array_buf = js_sys::Reflect::get(&data_view, &JsValue::from_str("buffer")).unwrap();
        //js_debug(&array_buf);

        let in_buf = js_sys::Uint8Array::new(&array_buf).to_vec();
        //log::info!("{:x?}",in_buf);
        (&mut buf[.. in_buf.len()]).copy_from_slice(&in_buf);
    
        Ok(in_buf.len())
    }


}

impl From<JsValue> for Error {
    fn from(e: JsValue) -> Self {
        if let Ok(domexp) = JsCast::dyn_into::<DomException>(e.clone()){
            match domexp.name().as_str() {
                "SecurityError" => Error::Security,
                "NotFoundError" => Error::NotSelected,
                _               => Error::DomExp(domexp),
            }
        } else {
            Error::RawJs(e)
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Self::Holter
    }
}

impl std::fmt::Display for Desc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}




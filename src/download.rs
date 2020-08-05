
use std::rc::Rc;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

use ellocopo2::owned::{Msg as DevMsg, Value};
use ellocopo2::AnswerCode;

use crate::device;
use crate::cmd;

#[wasm_bindgen(module = "/public/js/StreamSaver.js")]
extern "C" {

    #[wasm_bindgen]
    fn writeFile(filename: String, content: Vec<u8>) -> js_sys::Promise;

    type FileWriter;

    #[wasm_bindgen(constructor)]
    fn new(name: &str) -> FileWriter;

    #[wasm_bindgen(method)]
    fn write(this: &FileWriter, data: &[u8]) -> js_sys::Promise;

    #[wasm_bindgen(method)]
    fn close(this: &FileWriter);

    #[wasm_bindgen(method)]
    fn abort(this: &FileWriter);

}

pub async fn _test_download_file(filename: String, content: Vec<u8>) -> Result<(),JsValue> {

    // let promise: js_sys::Promise = writeFile(filename, content);
    // let result = wasm_bindgen_futures::JsFuture::from(promise).await?;

    let fw = FileWriter::new("tt.bin");
    let promise = fw.write(&[0xA5;4]);
    let result = wasm_bindgen_futures::JsFuture::from(promise).await?;

    let promise = fw.write(&[0xb4;2]);
    let result = wasm_bindgen_futures::JsFuture::from(promise).await?;

    let promise = fw.write(&[0xc7;4]);
    let result = wasm_bindgen_futures::JsFuture::from(promise).await?;

    fw.close();

    //crate::js_debug(&r);

    Ok(())
}

pub async fn download_file_from_device(device: Rc<device::Device>, filename: &str, cancel: &bool) -> Result<(),()> {

    log::info!("Performing download");

    async fn trans_start_cmds(device: Rc<device::Device>) -> Result<u32,()> {
        let _ = cmd(&device, DevMsg(AnswerCode::OK_WRITE, String::from("/io/file/pos"), Value::U32(0)))
            .await?;

        let len = cmd(&device, DevMsg(AnswerCode::OK_READ, String::from("/io/file/len"), Value::UNIT(())))
            .await?;

        let block_cnt = if let Value::U32(block_cnt) = len {
            40_000
        } else { unimplemented!() };

        let _ = cmd(&device, DevMsg(AnswerCode::OK_WRITE, String::from("/io/file/len"), Value::U32(block_cnt)))
            .await
            .unwrap();
    
        let _ = cmd(&device, DevMsg(AnswerCode::OK_WRITE, String::from("/io/file/start"), Value::UNIT(())))
            .await
            .unwrap();

        Ok(block_cnt)
    }

    let block_cnt = trans_start_cmds(device.clone())
        .await?;

    let file_writer = FileWriter::new(filename);

    for cnt in 0 .. block_cnt {
        let buf = device.recv_file_block().await.unwrap();
        log::info!("recv_file_block res {:?}", buf.len());
        log::info!("{:x?}", &buf[.. delta::defs::HEADER_SZ]);
        let mut parser = delta::block::parse::BlockParser::new();
        let r = parser.try_open_block(&buf[..]);
        log::info!("try_open_block res: {:?}", r);
        log::info!("Blk {} header: {:#?}", cnt, parser.header());
        // if buf.len() != 0x800 {
        //     log::error!("Wrong buf len {}", buf.len());
        // }

        let promise = file_writer.write(&buf);
        let _ = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|_| ())?;
    }

    file_writer.close();

    log::info!("End::Performing download");

    Ok(())
}
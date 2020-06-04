
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::collections::VecDeque;

use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{WebGlProgram, WebGlRenderingContext, WebGlShader, WebGlUniformLocation};
use serde::Serialize;


fn window() -> web_sys::Window {
    web_sys::window().expect("no global `window` exists")
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

pub fn vis_run(rx: mpsc::Receiver<Vec<i32>>) -> Result<(), JsValue> {
    
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;

    let height = 900;//canvas.offset_height() as u32;
    let width = canvas.offset_width() as u32;
    canvas.set_height(height);
    canvas.set_width(width);

    // Buf
    let mut buf = vec![VecDeque::<f32>::from(vec![0f32;width as usize * 2]);8];

    // Context settings
    #[derive(Serialize)]
    struct CxtCfg {
        antialias : bool,
        depth     : bool,
    };

    let cxt_cfg = CxtCfg { antialias : false, depth : false };
    let cxt_cfg = JsValue::from_serde(&cxt_cfg).unwrap();

    let context = canvas
        //.get_context("webgl")?
        .get_context_with_context_options("webgl", &cxt_cfg)?
        .unwrap()
        .dyn_into::<WebGlRenderingContext>()?;
        

    // Shaders
    let vert_shader = compile_shader(
        &context,
        WebGlRenderingContext::VERTEX_SHADER,
        r#"
        attribute vec2 a_position;
        uniform vec2 u_resolution;
        uniform vec2 u_shift;
        uniform vec2 u_xmod;

        void main() 
        {
          vec2 Pos = a_position + u_shift; 
          
          //if(u_xmod.x != 0.0 )
          //{
          //  Pos.x = mod(Pos.x,u_xmod.x);
          //};
          
          Pos.x = abs(Pos.x);
          Pos.x = mod(Pos.x, u_resolution.x);

          vec2 zeroToOne = Pos / u_resolution; // преобразуем положение в пикселях к диапазону от 0.0 до 1.0
       
          // преобразуем из 0->1 в 0->2
          vec2 zeroToTwo = zeroToOne * 2.0;
          // преобразуем из 0->2 в -1->+1 (пространство отсечения)
          vec2 clipSpace = zeroToTwo - 1.0;
          vec2 clipSpaceN = clipSpace * vec2(1, -1); // переворачиваем систему коооординат (0,0) в левом верхнем углу
     
          gl_Position = vec4(clipSpaceN, 0, 1);  
        }
    "#,
    )?;
    let frag_shader = compile_shader(
        &context,
        WebGlRenderingContext::FRAGMENT_SHADER,
        r#"
        void main() {
            gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        }
    "#,
    )?;
    let program = link_program(&context, &vert_shader, &frag_shader)?;
    context.use_program(Some(&program));

    //ATR
    //
    let position_attribute_location = context.get_attrib_location(&program, "a_position");
    let resolution_uniform_location = context.get_uniform_location(&program, "u_resolution");
    //context.get_uniform_location(&program, "u_color");
    let shift_location = context.get_uniform_location(&program, "u_shift");
    let xmod_location = context.get_uniform_location(&program, "u_xmod");

    let position_buffer = context.create_buffer().ok_or("failed to create buffer")?;
    context.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&position_buffer));

    context.enable_vertex_attrib_array(position_attribute_location as u32);
    context.vertex_attrib_pointer_with_i32(0, 2, WebGlRenderingContext::FLOAT, false, 0, 0);
    context.uniform2f(resolution_uniform_location.as_ref(), width as f32, height as f32);
    context.viewport(0,0, width as i32, height as i32);

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let mut cnt = 0usize;

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        match rx.try_recv() {
            Ok(sample) => {
                context.clear_color(1.0, 1.0, 1.0, 1.0);
                context.clear(WebGlRenderingContext::COLOR_BUFFER_BIT);

                for i in 0 .. sample.len() {
                    let _ = buf[i].pop_back();
                    let _ = buf[i].pop_back();
                    buf[i].push_front(sample[0] as f32);
                    buf[i].push_front(cnt as f32);
                    let (p1, p2) = buf[i].as_slices();

                    draw_plot_scrolling(
                        &context,
                        &xmod_location,
                        &shift_location,
                        -(cnt as f32),
                        100. + i as f32 * 100.0,
                        p1, p2
                    );
                }
                //context.finish();
                cnt = cnt.wrapping_add(1);
            }
            _ => (),
          }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));
    
    request_animation_frame(g.borrow().as_ref().unwrap());

    Ok(())
}

pub fn draw_plot_scrolling(
    context: &WebGlRenderingContext, 
    xmod_location: &Option<WebGlUniformLocation>, 
    shift_location: &Option<WebGlUniformLocation>,
    shift_h: f32,
    shift_v: f32,
    part1: &[f32],
    part2: &[f32],
) {
    if part1.len() != 0 {
        let vert_array1 = unsafe { js_sys::Float32Array::view(part1) };
        context.buffer_data_with_array_buffer_view(
            WebGlRenderingContext::ARRAY_BUFFER,
            &vert_array1,
            WebGlRenderingContext::STREAM_DRAW,
        );

        context.uniform2f(xmod_location.as_ref(), 0f32, 0f32);
        context.uniform2f(shift_location.as_ref(), shift_h, shift_v);
        context.draw_arrays(
            WebGlRenderingContext::LINE_STRIP,
            //WebGlRenderingContext::POINTS,
            0,
            part1.len() as i32 / 2,
        );
    }
    
    if part2.len() != 0 {
        let vert_array2 = unsafe { js_sys::Float32Array::view(part2) };
        context.buffer_data_with_array_buffer_view(
            WebGlRenderingContext::ARRAY_BUFFER,
            &vert_array2,
            WebGlRenderingContext::STREAM_DRAW,
        );

        context.uniform2f(xmod_location.as_ref(), 0f32, 0f32);
        context.uniform2f(shift_location.as_ref(), shift_h, shift_v);
        context.draw_arrays(
            WebGlRenderingContext::LINE_STRIP,
            //WebGlRenderingContext::POINTS,
            0,
            part2.len() as i32 / 2,
        );
    }
}

pub fn compile_shader(
    context: &WebGlRenderingContext,
    shader_type: u32,
    source: &str,
) 
    -> Result<WebGlShader, String> 
{
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGlRenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("Unknown error creating shader")))
    }
}

pub fn link_program(
    context: &WebGlRenderingContext,
    vert_shader: &WebGlShader,
    frag_shader: &WebGlShader,
) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create shader object"))?;

    context.attach_shader(&program, vert_shader);
    context.attach_shader(&program, frag_shader);
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGlRenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("Unknown error creating program object")))
    }
}



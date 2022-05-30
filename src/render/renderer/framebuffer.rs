use {
    crate::{
        fixed::Fixed,
        format::{Format, XRGB8888},
        rect::Rect,
        render::{
            gl::{
                frame_buffer::GlFrameBuffer,
                sys::{
                    glBindFramebuffer, glClear, glClearColor, glViewport, GL_COLOR_BUFFER_BIT,
                    GL_FRAMEBUFFER,
                },
            },
            renderer::{context::RenderContext, renderer::Renderer},
            sys::{glBlendFunc, glFlush, glReadnPixels, GL_ONE, GL_ONE_MINUS_SRC_ALPHA},
            RenderResult, Texture,
        },
        state::State,
        tree::Node,
    },
    std::{
        cell::Cell,
        fmt::{Debug, Formatter},
        rc::Rc,
    },
};

pub struct Framebuffer {
    pub(super) ctx: Rc<RenderContext>,
    pub(super) gl: GlFrameBuffer,
}

impl Debug for Framebuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Framebuffer").finish_non_exhaustive()
    }
}

impl Framebuffer {
    pub fn clear(&self) {
        let _ = self.ctx.ctx.with_current(|| {
            unsafe {
                glBindFramebuffer(GL_FRAMEBUFFER, self.gl.fbo);
                glViewport(0, 0, self.gl.width, self.gl.height);
                glClearColor(0.0, 0.0, 0.0, 0.0);
                glClear(GL_COLOR_BUFFER_BIT);
            }
            Ok(())
        });
    }

    pub fn copy_texture(&self, state: &State, texture: &Texture, x: i32, y: i32) {
        let _ = self.ctx.ctx.with_current(|| {
            unsafe {
                glBindFramebuffer(GL_FRAMEBUFFER, self.gl.fbo);
                glViewport(0, 0, self.gl.width, self.gl.height);
                glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
            }
            let scale = Fixed::from_int(1);
            let mut renderer = Renderer {
                ctx: &self.ctx,
                fb: &self.gl,
                state,
                on_output: false,
                result: &mut RenderResult::default(),
                scaled: false,
                scale,
                scalef: 1.0,
                logical_extents: Rect::new_sized(0, 0, self.gl.width, self.gl.height).unwrap(),
            };
            renderer.render_texture(texture, x, y, XRGB8888, None, None, scale);
            unsafe {
                glFlush();
            }
            Ok(())
        });
    }

    pub fn copy_to_shm(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        format: &Format,
        shm: &[Cell<u8>],
    ) {
        let y = self.gl.height - y - height;
        let _ = self.ctx.ctx.with_current(|| {
            unsafe {
                glBindFramebuffer(GL_FRAMEBUFFER, self.gl.fbo);
                glViewport(0, 0, self.gl.width, self.gl.height);
                glReadnPixels(
                    x,
                    y,
                    width,
                    height,
                    format.gl_format as _,
                    format.gl_type as _,
                    shm.len() as _,
                    shm.as_ptr() as _,
                );
            }
            Ok(())
        });
    }

    pub fn render(
        &self,
        node: &dyn Node,
        state: &State,
        cursor_rect: Option<Rect>,
        on_output: bool,
        result: &mut RenderResult,
        scale: Fixed,
    ) {
        let _ = self.ctx.ctx.with_current(|| {
            let c = state.theme.colors.background.get();
            unsafe {
                glBindFramebuffer(GL_FRAMEBUFFER, self.gl.fbo);
                glViewport(0, 0, self.gl.width, self.gl.height);
                glClearColor(c.r, c.g, c.b, 1.0);
                glClear(GL_COLOR_BUFFER_BIT);
                glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
            }
            let mut renderer = Renderer {
                ctx: &self.ctx,
                fb: &self.gl,
                state,
                on_output,
                result,
                scaled: scale != 1,
                scale,
                scalef: scale.to_f64(),
                logical_extents: node.node_absolute_position().at_point(0, 0),
            };
            node.node_render(&mut renderer, 0, 0);
            if let Some(rect) = cursor_rect {
                let seats = state.globals.lock_seats();
                for seat in seats.values() {
                    if let Some(cursor) = seat.get_cursor() {
                        let (mut x, mut y) = seat.get_position();
                        if let Some(dnd_icon) = seat.dnd_icon() {
                            let extents = dnd_icon.extents.get().move_(
                                x.round_down() + dnd_icon.buf_x.get(),
                                y.round_down() + dnd_icon.buf_y.get(),
                            );
                            if extents.intersects(&rect) {
                                let (x, y) = rect.translate(extents.x1(), extents.y1());
                                renderer.render_surface(&dnd_icon, x, y);
                            }
                        }
                        cursor.tick();
                        x -= Fixed::from_int(rect.x1());
                        y -= Fixed::from_int(rect.y1());
                        cursor.render(&mut renderer, x, y);
                    }
                }
            }
            unsafe {
                glFlush();
            }
            Ok(())
        });
    }
}

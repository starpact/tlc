use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    sync::Mutex,
};

use anyhow::Result;
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::Pixel::RGB24,
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use thread_local::ThreadLocal;

/// Decoder is bound to a specific video and can decode any packet of this video.
pub struct Decoder {
    codec_ctx: ffmpeg::decoder::Video,
    sws_ctx: SendableSwsCtx,
    /// `src_frame` and `dst_frame` are used to avoid frequent allocation.
    /// This can speed up decoding by about 10%.
    src_frame: Video,
    dst_frame: Video,
}

impl Decoder {
    fn new(parameters: Parameters) -> Result<Self> {
        let codec_ctx = codec::Context::from_parameters(parameters)?
            .decoder()
            .video()?;
        let (h, w) = (codec_ctx.height(), codec_ctx.width());
        let sws_ctx =
            scaling::Context::get(codec_ctx.format(), w, h, RGB24, w, h, Flags::BILINEAR)?;

        Ok(Self {
            codec_ctx,
            sws_ctx: SendableSwsCtx(sws_ctx),
            src_frame: Video::empty(),
            dst_frame: Video::empty(),
        })
    }

    pub fn decode(&mut self, packet: &Packet) -> Result<&Video> {
        self.codec_ctx.send_packet(packet)?;
        self.codec_ctx.receive_frame(&mut self.src_frame)?;
        self.sws_ctx.run(&self.src_frame, &mut self.dst_frame)?;

        Ok(&self.dst_frame)
    }
}

/// `DecoderManager` maintains a thread-local style decoder pool to avoid frequent
/// initialization of decoder. It should be used with thread pool with a small
/// number of threads so that the thread-local decoders can be reused efficiently.
#[derive(Default)]
pub struct DecoderManager {
    parameters: Mutex<Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,
}

impl DecoderManager {
    pub fn new(parameters: Parameters) -> DecoderManager {
        DecoderManager {
            parameters: Mutex::new(parameters),
            decoders: ThreadLocal::new(),
        }
    }

    pub fn decoder(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.parameters.lock().unwrap().clone())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }
}

/// Wrap `Context` to pass between threads(because of the raw pointer).
struct SendableSwsCtx(scaling::Context);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for SendableSwsCtx {}

impl Deref for SendableSwsCtx {
    type Target = scaling::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendableSwsCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

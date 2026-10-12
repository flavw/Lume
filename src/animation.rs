use image::{AnimationDecoder, RgbaImage};
use std::{path::Path, sync::Arc, time::Duration};

pub struct Animation {
    pub frames: Vec<(Arc<RgbaImage>, Duration)>,
    pub loops: Option<u32>,
}
pub fn load(path: &Path) -> Result<Animation, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let loops=loop_count(&data);
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut bytes = 0usize;
    for frame in decoder.into_frames() {
        let frame = frame.map_err(|e| e.to_string())?;
        bytes = bytes.checked_add(frame.buffer().as_raw().len()).ok_or("GIF grande demais.")?;
        if bytes > 256 * 1024 * 1024 || frames.len() >= 2000 { return Err("GIF grande demais.".into()); }
        let (ms, denominator) = frame.delay().numer_denom_ms();
        // Zero delays are common; clamp only those to avoid a busy redraw loop.
        let delay = Duration::from_secs_f64((f64::from(ms) / f64::from(denominator.max(1)) / 1000.0).max(0.01));
        frames.push((Arc::new(frame.into_buffer()),delay));
    }
    if frames.is_empty() { return Err("GIF sem quadros.".into()); }
    Ok(Animation { frames, loops })
}

fn loop_count(data: &[u8]) -> Option<u32> {
    let mut pos=13usize;
    if let Some(packed)=data.get(10) {if packed & 0x80 != 0 {pos += 3*(1usize << ((packed & 7)+1));}}
    while let Some(block)=data.get(pos) {
        pos+=1;
        match block {
            0x21 => {
                let kind=*data.get(pos)?;pos+=1;
                if kind==0xff && data.get(pos)==Some(&11) {
                    let app=data.get(pos+1..pos+12)?;
                    if app==b"NETSCAPE2.0" || app==b"ANIMEXTS1.0" {
                        let block=data.get(pos+12..pos+17)?;
                        if block[0]==3 && block[1]==1 {
                            let repeat=u16::from_le_bytes([block[2],block[3]]);
                            return if repeat==0 {None} else {Some(u32::from(repeat)+1)};
                        }
                    }
                }
            }
            0x2c => {
                let packed=*data.get(pos+8)?;pos+=9;
                if packed & 0x80 != 0 {pos+=3*(1usize << ((packed&7)+1));}
                pos+=1;
            }
            _ => break,
        }
        loop {let size=usize::from(*data.get(pos)?);pos+=1;if size==0 {break;}pos=pos.checked_add(size)?;}
    }
    Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn animated_frames_and_delays_are_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("animated.gif");
        let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        encoder.encode_frames([image::Frame::from_parts(RgbaImage::from_pixel(2,2,image::Rgba([255,0,0,255])),0,0,image::Delay::from_numer_denom_ms(100,1)),image::Frame::from_parts(RgbaImage::from_pixel(2,2,image::Rgba([0,255,0,255])),0,0,image::Delay::from_numer_denom_ms(200,1))].into_iter()).unwrap();
        drop(encoder);
        let a = load(&path).unwrap();
        assert_eq!(a.frames.len(),2);
        assert_eq!(a.loops,Some(1));
        assert_eq!(a.frames[0].1,Duration::from_millis(100));
        assert_eq!(a.frames[1].1,Duration::from_millis(200));
        assert_eq!(*a.frames[1].0.get_pixel(0,0),image::Rgba([0,255,0,255]));
    }
    #[test]
    fn loop_metadata_is_respected() {
        let dir=tempfile::tempdir().unwrap();
        for (repeat,expected) in [(image::codecs::gif::Repeat::Infinite,None),(image::codecs::gif::Repeat::Finite(2),Some(3))] {
            let path=dir.path().join("repeat.gif");
            let mut encoder=image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
            encoder.set_repeat(repeat).unwrap();
            encoder.encode_frame(image::Frame::new(RgbaImage::from_pixel(2,2,image::Rgba([0,255,0,255])))).unwrap();
            drop(encoder);
            assert_eq!(load(&path).unwrap().loops,expected);
        }
    }
}

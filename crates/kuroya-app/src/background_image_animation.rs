use crate::{
    file_io::{FILE_OPEN_MAX_BYTES, open_read_file_with_limit_sync},
    image_preview::LoadedImagePreview,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
use eframe::egui::Context;
use image::{
    AnimationDecoder, Delay, Frame, ImageDecoder, Limits, codecs::gif::GifDecoder,
    metadata::LoopCount,
};
use std::{
    io::BufReader,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

const GIF_FRAME_CHANNEL_CAPACITY: usize = 1;
const MAX_ANIMATED_BACKGROUND_PIXELS: u64 = 1920 * 1080;
const GIF_RGBA_BYTES_PER_PIXEL: u64 = 4;
const GIF_DECODER_LIVE_CANVAS_COUNT: u64 = 3;
const MAX_GIF_DECODER_ALLOC_BYTES: u64 =
    MAX_ANIMATED_BACKGROUND_PIXELS * GIF_RGBA_BYTES_PER_PIXEL * GIF_DECODER_LIVE_CANVAS_COUNT;
const MIN_GIF_FRAME_DELAY: Duration = Duration::from_millis(16);

const BROWSER_FAST_GIF_DELAY_THRESHOLD_MS: f64 = 10.0;
const BROWSER_FAST_GIF_FRAME_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct LoadedGifBackground {
    pub(crate) first_frame: LoadedImagePreview,
    pub(crate) first_frame_delay: Duration,
    pub(crate) animation: BackgroundGifAnimation,
}

#[derive(Debug)]
pub(crate) struct BackgroundGifAnimation {
    requests: Sender<()>,
    frames: Receiver<BackgroundGifAnimationEvent>,
    next_frame_at: Option<Instant>,
    frame_delay: Duration,
    request_in_flight: bool,
}

#[derive(Debug)]
pub(crate) enum BackgroundGifAnimationUpdate {
    Frame(LoadedImagePreview),
    Finished,
    Failed(String),
}

#[derive(Debug)]
enum BackgroundGifAnimationEvent {
    Frame(DecodedGifFrame),
    Finished,
    Failed(String),
}

#[derive(Debug)]
struct DecodedGifFrame {
    preview: LoadedImagePreview,
    delay: Duration,
}

impl BackgroundGifAnimation {
    pub(crate) fn activate(
        &mut self,
        first_frame_delay: Duration,
        ctx: Option<&Context>,
        paused: bool,
    ) {
        self.schedule_next_frame(first_frame_delay, ctx, paused);
    }

    pub(crate) fn poll(
        &mut self,
        ctx: &Context,
        paused: bool,
    ) -> Option<BackgroundGifAnimationUpdate> {
        match self.frames.try_recv() {
            Ok(BackgroundGifAnimationEvent::Frame(frame)) => {
                self.request_in_flight = false;
                self.schedule_next_frame(frame.delay, Some(ctx), paused);
                return Some(BackgroundGifAnimationUpdate::Frame(frame.preview));
            }
            Ok(BackgroundGifAnimationEvent::Finished) => {
                return Some(BackgroundGifAnimationUpdate::Finished);
            }
            Ok(BackgroundGifAnimationEvent::Failed(error)) => {
                return Some(BackgroundGifAnimationUpdate::Failed(error));
            }
            Err(TryRecvError::Disconnected) => {
                return Some(BackgroundGifAnimationUpdate::Finished);
            }
            Err(TryRecvError::Empty) => {}
        }

        if paused {
            self.next_frame_at = None;
            return None;
        }
        if self.request_in_flight {
            return None;
        }

        let next_frame_at = match self.next_frame_at {
            Some(next_frame_at) => next_frame_at,
            None => {
                self.schedule_next_frame(self.frame_delay, Some(ctx), false);
                self.next_frame_at
                    .expect("scheduling on resume sets a fresh deadline")
            }
        };
        let now = Instant::now();
        if next_frame_at > now {
            ctx.request_repaint_after(next_frame_at.duration_since(now));
            return None;
        }

        match self.requests.try_send(()) {
            Ok(()) => self.request_in_flight = true,
            Err(TrySendError::Full(())) => self.request_in_flight = true,
            Err(TrySendError::Disconnected(())) => {
                return Some(BackgroundGifAnimationUpdate::Finished);
            }
        }
        None
    }

    fn schedule_next_frame(&mut self, delay: Duration, ctx: Option<&Context>, paused: bool) {
        self.frame_delay = delay.max(MIN_GIF_FRAME_DELAY);
        if paused {
            self.next_frame_at = None;
            return;
        }
        let now = Instant::now();
        let next_frame_at = match self.next_frame_at {
            Some(previous) => previous
                .checked_add(self.frame_delay)
                .map(|advanced| advanced.max(now)),
            None => now.checked_add(self.frame_delay),
        };
        self.next_frame_at = next_frame_at;
        if let Some(next_frame_at) = next_frame_at
            && let Some(ctx) = ctx
        {
            ctx.request_repaint_after(next_frame_at.saturating_duration_since(now));
        }
    }
}

pub(crate) fn path_is_animated_gif(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
}

pub(crate) async fn load_animated_gif_background(
    path: &Path,
    repaint_context: Option<Context>,
) -> Result<LoadedGifBackground, String> {
    let (requests_tx, requests_rx) = bounded(GIF_FRAME_CHANNEL_CAPACITY);
    let (frames_tx, frames_rx) = bounded(GIF_FRAME_CHANNEL_CAPACITY);
    let (first_frame_tx, first_frame_rx) = oneshot::channel();
    let worker_path = path.to_path_buf();

    thread::Builder::new()
        .name("kuroya-background-gif".to_owned())
        .spawn(move || {
            run_gif_worker(
                worker_path,
                first_frame_tx,
                requests_rx,
                frames_tx,
                repaint_context,
            );
        })
        .map_err(|error| format!("could not start animated GIF decoder: {error}"))?;

    let first_frame = first_frame_rx
        .await
        .map_err(|_| "animated GIF decoder stopped before the first frame".to_owned())??;
    Ok(LoadedGifBackground {
        first_frame: first_frame.preview,
        first_frame_delay: first_frame.delay,
        animation: BackgroundGifAnimation {
            requests: requests_tx,
            frames: frames_rx,
            next_frame_at: None,
            frame_delay: MIN_GIF_FRAME_DELAY,
            request_in_flight: false,
        },
    })
}

fn run_gif_worker(
    path: PathBuf,
    first_frame_tx: oneshot::Sender<Result<DecodedGifFrame, String>>,
    requests: Receiver<()>,
    frames: Sender<BackgroundGifAnimationEvent>,
    repaint_context: Option<Context>,
) {
    let mut first_frame_tx = Some(first_frame_tx);
    let mut first_frame_sent = false;
    let mut carry_advance_request = true;
    let mut completed_plays = 0_u32;
    let mut finite_plays = None;

    loop {
        let (decoder, source_byte_len) = match open_gif_decoder(&path) {
            Ok(loaded) => loaded,
            Err(error) => {
                send_worker_failure(
                    &mut first_frame_tx,
                    &frames,
                    repaint_context.as_ref(),
                    error,
                );
                return;
            }
        };
        finite_plays.get_or_insert_with(|| finite_loop_count(decoder.loop_count()));
        let mut decoded_frames = decoder.into_frames();
        let mut frames_in_play = 0_u64;

        loop {
            if first_frame_sent && !carry_advance_request && requests.recv().is_err() {
                return;
            }
            carry_advance_request = false;

            let frame = match decoded_frames.next() {
                Some(Ok(frame)) => frame,
                Some(Err(error)) => {
                    send_worker_failure(
                        &mut first_frame_tx,
                        &frames,
                        repaint_context.as_ref(),
                        format!("could not decode animated GIF frame: {error}"),
                    );
                    return;
                }
                None => {
                    carry_advance_request = true;
                    break;
                }
            };
            frames_in_play = frames_in_play.saturating_add(1);
            let frame = match decoded_gif_frame(frame, source_byte_len) {
                Ok(frame) => frame,
                Err(error) => {
                    send_worker_failure(
                        &mut first_frame_tx,
                        &frames,
                        repaint_context.as_ref(),
                        error,
                    );
                    return;
                }
            };

            if !first_frame_sent {
                let Some(sender) = first_frame_tx.take() else {
                    return;
                };
                if sender.send(Ok(frame)).is_err() {
                    return;
                }
                first_frame_sent = true;
                continue;
            }

            if !send_animation_event(
                &frames,
                repaint_context.as_ref(),
                BackgroundGifAnimationEvent::Frame(frame),
            ) {
                return;
            }
        }

        completed_plays = completed_plays.saturating_add(1);
        let play_limit_reached = finite_plays
            .flatten()
            .is_some_and(|play_limit| completed_plays >= play_limit);
        if frames_in_play <= 1 || play_limit_reached {
            send_animation_event(
                &frames,
                repaint_context.as_ref(),
                BackgroundGifAnimationEvent::Finished,
            );
            return;
        }
    }
}

fn open_gif_decoder(path: &Path) -> Result<(GifDecoder<BufReader<std::fs::File>>, usize), String> {
    let (file, source_byte_len) = open_read_file_with_limit_sync(path, FILE_OPEN_MAX_BYTES)?;
    let mut decoder = GifDecoder::new(BufReader::new(file))
        .map_err(|error| format!("could not open animated GIF: {error}"))?;
    let (width, height) = decoder.dimensions();
    validate_gif_dimensions(width, height)?;

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_GIF_DECODER_ALLOC_BYTES);
    decoder
        .set_limits(limits)
        .map_err(|error| format!("animated GIF exceeds decoder limits: {error}"))?;

    Ok((
        decoder,
        usize::try_from(source_byte_len).unwrap_or(usize::MAX),
    ))
}

fn validate_gif_dimensions(width: u32, height: u32) -> Result<(), String> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 {
        return Err("animated GIF has invalid zero-sized dimensions".to_owned());
    }
    if pixels > MAX_ANIMATED_BACKGROUND_PIXELS {
        return Err(format!(
            "animated GIF has {pixels} pixels; the background limit is {MAX_ANIMATED_BACKGROUND_PIXELS}"
        ));
    }
    Ok(())
}

fn decoded_gif_frame(frame: Frame, source_byte_len: usize) -> Result<DecodedGifFrame, String> {
    let delay = bounded_gif_frame_delay(frame.delay());
    let rgba = frame.into_buffer();
    let (width, height) = rgba.dimensions();
    validate_gif_dimensions(width, height)?;
    Ok(DecodedGifFrame {
        preview: LoadedImagePreview {
            width: width as usize,
            height: height as usize,
            rgba: Some(rgba.into_raw()),
            byte_len: source_byte_len,
        },
        delay,
    })
}

fn bounded_gif_frame_delay(delay: Delay) -> Duration {
    let (numerator, denominator) = delay.numer_denom_ms();
    let milliseconds = f64::from(numerator) / f64::from(denominator);
    if milliseconds <= BROWSER_FAST_GIF_DELAY_THRESHOLD_MS {
        return BROWSER_FAST_GIF_FRAME_DELAY;
    }
    Duration::from_secs_f64(milliseconds / 1000.0).max(MIN_GIF_FRAME_DELAY)
}

fn finite_loop_count(loop_count: LoopCount) -> Option<u32> {
    match loop_count {
        LoopCount::Infinite => None,
        LoopCount::Finite(count) => Some(count.get()),
    }
}

fn send_worker_failure(
    first_frame_tx: &mut Option<oneshot::Sender<Result<DecodedGifFrame, String>>>,
    frames: &Sender<BackgroundGifAnimationEvent>,
    repaint_context: Option<&Context>,
    error: String,
) {
    if let Some(sender) = first_frame_tx.take() {
        let _ = sender.send(Err(error));
        return;
    }
    send_animation_event(
        frames,
        repaint_context,
        BackgroundGifAnimationEvent::Failed(error),
    );
}

fn send_animation_event(
    frames: &Sender<BackgroundGifAnimationEvent>,
    repaint_context: Option<&Context>,
    event: BackgroundGifAnimationEvent,
) -> bool {
    if frames.send(event).is_err() {
        return false;
    }
    if let Some(ctx) = repaint_context {
        ctx.request_repaint();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundGifAnimation, BackgroundGifAnimationUpdate, MAX_ANIMATED_BACKGROUND_PIXELS,
        MIN_GIF_FRAME_DELAY, bounded_gif_frame_delay, load_animated_gif_background,
        path_is_animated_gif, validate_gif_dimensions,
    };
    use crossbeam_channel::bounded;
    use eframe::egui;
    use image::{
        Delay, Frame, Rgba, RgbaImage,
        codecs::gif::{GifEncoder, Repeat},
    };
    use std::{
        fs::File,
        path::{Path, PathBuf},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn gif_background_detection_is_case_insensitive() {
        assert!(path_is_animated_gif(Path::new("wallpaper.GIF")));
        assert!(!path_is_animated_gif(Path::new("wallpaper.png")));
    }

    #[test]
    fn gif_frame_delay_follows_browser_conventions() {
        assert_eq!(
            bounded_gif_frame_delay(Delay::from_numer_denom_ms(0, 1)),
            Duration::from_millis(100)
        );
        assert_eq!(
            bounded_gif_frame_delay(Delay::from_numer_denom_ms(8, 1)),
            Duration::from_millis(100)
        );

        assert_eq!(
            bounded_gif_frame_delay(Delay::from_numer_denom_ms(20, 1)),
            Duration::from_millis(20)
        );
        assert_eq!(
            bounded_gif_frame_delay(Delay::from_numer_denom_ms(12, 1)),
            MIN_GIF_FRAME_DELAY
        );
    }

    #[test]
    fn gif_frame_pacing_advances_on_a_virtual_clock_without_drift() {
        let (mut animation, _frames_tx) = test_gif_animation();
        let delay = Duration::from_millis(200);

        let anchor = Instant::now()
            .checked_sub(Duration::from_millis(5))
            .expect("clock supports subtraction");

        animation.next_frame_at = Some(anchor);
        animation.schedule_next_frame(delay, None, false);
        assert_eq!(
            animation.next_frame_at,
            Some(anchor + delay),
            "the deadline must advance from the previous deadline, not receive time"
        );

        animation.schedule_next_frame(delay, None, false);
        assert_eq!(animation.next_frame_at, Some(anchor + delay * 2));
    }

    #[test]
    fn overdue_gif_frames_fire_immediately_without_bursting() {
        let (mut animation, _frames_tx) = test_gif_animation();
        let delay = Duration::from_millis(20);
        let overdue = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .expect("clock supports subtraction");

        animation.next_frame_at = Some(overdue);
        animation.schedule_next_frame(delay, None, false);

        let scheduled = animation
            .next_frame_at
            .expect("an overdue deadline is rescheduled");
        assert!(
            scheduled >= overdue + delay,
            "the deadline still advances by one delay"
        );
        assert!(
            scheduled <= Instant::now(),
            "an overdue deadline must fire immediately instead of re-anchoring to now"
        );
    }

    #[test]
    fn pausing_cancels_the_deadline_and_resume_honors_a_full_delay() {
        let (mut animation, _frames_tx) = test_gif_animation();
        let ctx = egui::Context::default();

        animation.activate(Duration::from_millis(100), Some(&ctx), true);
        assert_eq!(
            animation.next_frame_at, None,
            "pausing must cancel the pending frame deadline"
        );
        assert!(animation.poll(&ctx, true).is_none());

        assert!(animation.poll(&ctx, false).is_none());
        assert!(!animation.request_in_flight);
        let resumed_deadline = animation
            .next_frame_at
            .expect("resuming schedules a fresh deadline");
        assert!(
            resumed_deadline > Instant::now(),
            "resume must honor the full frame delay"
        );
    }

    fn test_gif_animation() -> (
        BackgroundGifAnimation,
        crossbeam_channel::Sender<super::BackgroundGifAnimationEvent>,
    ) {
        let (requests, _requests_rx) = bounded(1);
        let (frames_tx, frames) = bounded(1);
        let animation = BackgroundGifAnimation {
            requests,
            frames,
            next_frame_at: None,
            frame_delay: MIN_GIF_FRAME_DELAY,
            request_in_flight: false,
        };
        (animation, frames_tx)
    }

    #[test]
    fn animated_gif_dimensions_have_a_bounded_canvas() {
        assert!(validate_gif_dimensions(1920, 1080).is_ok());
        assert!(validate_gif_dimensions(0, 1).is_err());
        assert!(
            validate_gif_dimensions(u32::try_from(MAX_ANIMATED_BACKGROUND_PIXELS).unwrap(), 2,)
                .is_err()
        );
    }

    #[tokio::test]
    async fn animated_gif_streams_one_requested_frame_at_a_time() {
        let path = temp_gif_path("stream");
        write_two_frame_gif(&path);
        let mut loaded = load_animated_gif_background(&path, None)
            .await
            .expect("animated GIF should load");

        assert_eq!(loaded.first_frame.width, 2);
        assert_eq!(loaded.first_frame.height, 1);
        assert_eq!(
            loaded.first_frame.rgba.as_deref(),
            Some(&[255, 0, 0, 255, 255, 0, 0, 255][..])
        );

        let ctx = egui::Context::default();
        loaded.animation.activate(Duration::ZERO, Some(&ctx), false);
        std::thread::sleep(MIN_GIF_FRAME_DELAY + Duration::from_millis(5));
        assert!(loaded.animation.poll(&ctx, false).is_none());

        let deadline = Instant::now() + Duration::from_secs(2);
        let second_frame = loop {
            match loaded.animation.poll(&ctx, false) {
                Some(BackgroundGifAnimationUpdate::Frame(preview)) => break preview,
                Some(other) => panic!("unexpected animation update: {other:?}"),
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                None => panic!("timed out waiting for the second GIF frame"),
            }
        };
        assert_eq!(
            second_frame.rgba.as_deref(),
            Some(&[0, 255, 0, 255, 0, 255, 0, 255][..])
        );

        drop(loaded);
        let cleanup_deadline = Instant::now() + Duration::from_secs(1);
        while std::fs::remove_file(&path).is_err() && Instant::now() < cleanup_deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn minimized_animation_does_not_request_another_frame() {
        let path = temp_gif_path("paused");
        write_two_frame_gif(&path);
        let mut loaded = load_animated_gif_background(&path, None)
            .await
            .expect("animated GIF should load");
        let ctx = egui::Context::default();
        loaded.animation.activate(Duration::ZERO, Some(&ctx), true);
        std::thread::sleep(MIN_GIF_FRAME_DELAY + Duration::from_millis(5));

        assert!(loaded.animation.poll(&ctx, true).is_none());
        assert!(!loaded.animation.request_in_flight);

        drop(loaded);
        let cleanup_deadline = Instant::now() + Duration::from_secs(1);
        while std::fs::remove_file(&path).is_err() && Instant::now() < cleanup_deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!path.exists());
    }

    fn write_two_frame_gif(path: &Path) {
        let file = File::create(path).expect("test GIF should be created");
        let mut encoder = GifEncoder::new(file);
        encoder
            .set_repeat(Repeat::Infinite)
            .expect("repeat mode should encode");
        for color in [Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255])] {
            let frame = Frame::from_parts(
                RgbaImage::from_pixel(2, 1, color),
                0,
                0,
                Delay::from_numer_denom_ms(1, 1),
            );
            encoder
                .encode_frame(frame)
                .expect("test GIF frame should encode");
        }
    }

    fn temp_gif_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-background-{label}-{}-{nonce}.gif",
            std::process::id()
        ))
    }
}

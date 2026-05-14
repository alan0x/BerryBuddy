use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use ai_foundation::providers::ark::{ArkRealtimeConfig, ArkRealtimeProvider, ArkRealtimeSession};
use ai_foundation::providers::router::ProviderRouter;
use ai_foundation::{
    Provider, RealtimeConnectRequest, RealtimeConversationSession, RealtimeServerEvent,
    RealtimeStartSessionRequest,
};
use anyhow::{Context, Result, anyhow, bail};
use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use tokio::sync::mpsc;

const ARK_INPUT_SAMPLE_RATE: u32 = 16_000;
const ARK_TTS_SAMPLE_RATE: u32 = 24_000;
const INPUT_CHUNK_MS: u32 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let settings = Settings::from_env()?;
    if settings.list_devices {
        list_audio_devices()?;
        return Ok(());
    }

    println!("BerryBuddy starting realtime voice chat.");
    println!(
        "Input sample rate sent to Ark: {} Hz",
        ARK_INPUT_SAMPLE_RATE
    );
    println!(
        "TTS sample rate expected from Ark: {} Hz",
        ARK_TTS_SAMPLE_RATE
    );

    let realtime_provider = Arc::new(ArkRealtimeProvider::new(settings.ark_realtime_config()));
    let router = ProviderRouter::new().with_realtime_provider(Provider::Ark, realtime_provider);
    let session = router
        .connect_realtime_conversation(
            Provider::Ark,
            RealtimeConnectRequest::new().with_connect_id(settings.connect_id.clone()),
        )
        .await
        .context("failed to connect Ark realtime conversation")?;

    wait_for_connection_started(session.as_ref()).await?;

    let session_id = ArkRealtimeSession::new_session_id();
    session
        .start_session(settings.start_session_request(&session_id))
        .await
        .context("failed to start Ark realtime session")?;
    wait_for_session_started(session.as_ref()).await?;

    let playback = AudioPlayback::start(settings.output_device.as_deref(), ARK_TTS_SAMPLE_RATE)?;
    let (audio_tx, audio_rx) = mpsc::channel::<Bytes>(settings.audio_queue_size);
    let input_stream = start_microphone(settings.input_device.as_deref(), audio_tx)?;
    let audio_task = spawn_audio_sender(session.clone(), session_id.clone(), audio_rx);

    println!("Realtime voice chat is active. Speak into the microphone.");
    println!("Press Ctrl+C to stop.");

    run_event_loop(session.clone(), &playback, settings.verbose_events).await?;

    drop(input_stream);
    audio_task.abort();
    playback.clear();

    let _ = session.finish_session(session_id).await;
    let _ = session.close().await;
    println!("BerryBuddy stopped.");
    Ok(())
}

#[derive(Debug, Clone)]
struct Settings {
    ark_app_id: String,
    ark_access_key: String,
    ark_endpoint: Option<String>,
    ark_resource_id: Option<String>,
    ark_app_key: Option<String>,
    connect_id: String,
    model: String,
    bot_name: String,
    system_role: String,
    speaking_style: String,
    speaker: String,
    asr_format: String,
    input_mode: Option<String>,
    input_device: Option<String>,
    output_device: Option<String>,
    audio_queue_size: usize,
    verbose_events: bool,
    list_devices: bool,
}

impl Settings {
    fn from_env() -> Result<Self> {
        let list_devices = env_bool("BERRYBUDDY_LIST_DEVICES", false)?;
        if list_devices {
            return Ok(Self::device_listing_only());
        }

        let ark_app_id = env_first(&["ARK_APP_ID", "ARK_ID"])
            .context("set ARK_APP_ID in .env or environment")?;
        let ark_access_key = env_first(&["ARK_ACCESS_KEY", "ARK_API_KEY"])
            .context("set ARK_ACCESS_KEY in .env or environment")?;

        Ok(Self {
            ark_app_id,
            ark_access_key,
            ark_endpoint: env_optional("ARK_REALTIME_ENDPOINT"),
            ark_resource_id: env_optional("ARK_RESOURCE_ID"),
            ark_app_key: env_optional("ARK_APP_KEY"),
            connect_id: env_optional("BERRYBUDDY_CONNECT_ID")
                .unwrap_or_else(|| format!("berrybuddy-{}", std::process::id())),
            model: env_default("BERRYBUDDY_MODEL", "1.2.1.1"),
            bot_name: env_default("BERRYBUDDY_BOT_NAME", "BerryBuddy"),
            system_role: env_default(
                "BERRYBUDDY_SYSTEM_ROLE",
                "You are BerryBuddy, a warm and concise voice companion for a child. Keep replies short, safe, and friendly.",
            ),
            speaking_style: env_default(
                "BERRYBUDDY_SPEAKING_STYLE",
                "Use simple words and speak naturally in short sentences.",
            ),
            speaker: env_default("BERRYBUDDY_SPEAKER", "zh_female_vv_jupiter_bigtts"),
            asr_format: env_default("BERRYBUDDY_ASR_FORMAT", "pcm"),
            input_mode: env_optional("BERRYBUDDY_INPUT_MODE"),
            input_device: env_optional("BERRYBUDDY_INPUT_DEVICE"),
            output_device: env_optional("BERRYBUDDY_OUTPUT_DEVICE"),
            audio_queue_size: env_parse("BERRYBUDDY_AUDIO_QUEUE_SIZE", 128)?,
            verbose_events: env_bool("BERRYBUDDY_VERBOSE_EVENTS", false)?,
            list_devices,
        })
    }

    fn device_listing_only() -> Self {
        Self {
            ark_app_id: String::new(),
            ark_access_key: String::new(),
            ark_endpoint: None,
            ark_resource_id: None,
            ark_app_key: None,
            connect_id: String::new(),
            model: "1.2.1.1".to_string(),
            bot_name: "BerryBuddy".to_string(),
            system_role: String::new(),
            speaking_style: String::new(),
            speaker: "zh_female_vv_jupiter_bigtts".to_string(),
            asr_format: "pcm".to_string(),
            input_mode: None,
            input_device: None,
            output_device: None,
            audio_queue_size: 128,
            verbose_events: false,
            list_devices: true,
        }
    }

    fn ark_realtime_config(&self) -> ArkRealtimeConfig {
        let mut config =
            ArkRealtimeConfig::new(self.ark_app_id.clone(), self.ark_access_key.clone());
        if let Some(endpoint) = &self.ark_endpoint {
            config = config.with_endpoint(endpoint.clone());
        }
        if let Some(resource_id) = &self.ark_resource_id {
            config = config.with_resource_id(resource_id.clone());
        }
        if let Some(app_key) = &self.ark_app_key {
            config = config.with_app_key(app_key.clone());
        }
        config
    }

    fn start_session_request(&self, session_id: &str) -> RealtimeStartSessionRequest {
        let mut payload = serde_json::json!({
            "tts": {
                "speaker": self.speaker,
                "extra": {},
                "audio_config": {
                    "channel": 1,
                    "format": "pcm_s16le",
                    "sample_rate": ARK_TTS_SAMPLE_RATE
                }
            },
            "asr": {
                "extra": {},
                "audio_info": {
                    "format": self.asr_format,
                    "sample_rate": ARK_INPUT_SAMPLE_RATE,
                    "channel": 1
                }
            },
            "dialog": {
                "bot_name": self.bot_name,
                "system_role": self.system_role,
                "speaking_style": self.speaking_style,
                "dialog_id": "",
                "extra": {
                    "model": self.model
                }
            }
        });

        if let Some(input_mode) = &self.input_mode {
            payload["dialog"]["extra"]["input_mod"] = serde_json::Value::String(input_mode.clone());
        }

        RealtimeStartSessionRequest::from_payload(session_id, payload)
    }
}

async fn wait_for_connection_started(session: &dyn RealtimeConversationSession) -> Result<()> {
    loop {
        match session.next_event().await? {
            Some(RealtimeServerEvent::ConnectionStarted { .. }) => {
                println!("Ark realtime connection started.");
                return Ok(());
            }
            Some(RealtimeServerEvent::ConnectionFailed { error, .. }) => {
                bail!(
                    "Ark realtime connection failed: {}",
                    error.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            Some(RealtimeServerEvent::Error { message, .. }) => {
                bail!(
                    "Ark realtime connection error: {}",
                    message.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            Some(event) => {
                println!("Waiting for connection, received: {}", event_name(&event));
            }
            None => bail!("Ark realtime websocket closed before connection started"),
        }
    }
}

async fn wait_for_session_started(session: &dyn RealtimeConversationSession) -> Result<()> {
    loop {
        match session.next_event().await? {
            Some(RealtimeServerEvent::SessionStarted { dialog_id, .. }) => {
                println!(
                    "Ark realtime session started. dialog_id={}",
                    dialog_id.unwrap_or_default()
                );
                return Ok(());
            }
            Some(RealtimeServerEvent::SessionFailed { error, .. }) => {
                bail!(
                    "Ark realtime session failed: {}",
                    error.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            Some(RealtimeServerEvent::DialogCommonError { message, .. }) => {
                bail!(
                    "Ark realtime session error: {}",
                    message.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            Some(RealtimeServerEvent::Error { message, .. }) => {
                bail!(
                    "Ark realtime session error: {}",
                    message.unwrap_or_else(|| "unknown error".to_string())
                );
            }
            Some(event) => {
                println!("Waiting for session, received: {}", event_name(&event));
            }
            None => bail!("Ark realtime websocket closed before session started"),
        }
    }
}

fn spawn_audio_sender(
    session: Arc<dyn RealtimeConversationSession>,
    session_id: String,
    mut audio_rx: mpsc::Receiver<Bytes>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(chunk) = audio_rx.recv().await {
            if let Err(error) = session.send_audio(session_id.clone(), chunk).await {
                eprintln!("failed to send microphone audio to Ark: {error:#}");
                break;
            }
        }
    })
}

async fn run_event_loop(
    session: Arc<dyn RealtimeConversationSession>,
    playback: &AudioPlayback,
    verbose_events: bool,
) -> Result<()> {
    loop {
        tokio::select! {
            event = session.next_event() => {
                let Some(event) = event? else {
                    println!("Ark realtime websocket closed.");
                    return Ok(());
                };
                if !handle_server_event(event, playback, verbose_events)? {
                    return Ok(());
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to listen for Ctrl+C")?;
                println!("Stopping...");
                return Ok(());
            }
        }
    }
}

fn handle_server_event(
    event: RealtimeServerEvent,
    playback: &AudioPlayback,
    verbose_events: bool,
) -> Result<bool> {
    match event {
        RealtimeServerEvent::AsrInfo { question_id, .. } => {
            playback.clear();
            println!();
            println!(
                "User started speaking. question_id={}",
                question_id.unwrap_or_default()
            );
        }
        RealtimeServerEvent::AsrResponse { results, .. } => {
            for result in results {
                if result.is_interim {
                    println!("ASR interim: {}", result.text);
                } else {
                    println!("ASR final: {}", result.text);
                }
            }
        }
        RealtimeServerEvent::AsrEnded { .. } => {
            println!("ASR ended.");
        }
        RealtimeServerEvent::ChatResponse { content, .. } => {
            if let Some(content) = content {
                print!("{content}");
                io::stdout().flush().ok();
            }
        }
        RealtimeServerEvent::ChatEnded { .. } => {
            println!();
        }
        RealtimeServerEvent::TtsSentenceStart { text, .. } => {
            if let Some(text) = text {
                println!("TTS: {text}");
            }
        }
        RealtimeServerEvent::TtsResponse { audio, .. } => {
            playback.enqueue_pcm_s16le(&audio)?;
        }
        RealtimeServerEvent::TtsEnded { status_code, .. } => {
            if let Some(status_code) = status_code {
                println!("TTS ended. status_code={status_code}");
            }
        }
        RealtimeServerEvent::SessionFinished { .. }
        | RealtimeServerEvent::ConnectionFinished { .. } => {
            println!("Ark session finished.");
            return Ok(false);
        }
        RealtimeServerEvent::SessionFailed { error, .. } => {
            bail!(
                "Ark session failed: {}",
                error.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        RealtimeServerEvent::ConnectionFailed { error, .. } => {
            bail!(
                "Ark connection failed: {}",
                error.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        RealtimeServerEvent::DialogCommonError {
            status_code,
            message,
            ..
        } => {
            eprintln!(
                "Ark dialog error status={} message={}",
                status_code.unwrap_or_default(),
                message.unwrap_or_default()
            );
        }
        RealtimeServerEvent::Error {
            code,
            message,
            payload,
            ..
        } => {
            eprintln!(
                "Ark realtime error code={:?} message={} payload_bytes={}",
                code,
                message.unwrap_or_default(),
                payload.len()
            );
        }
        other if verbose_events => {
            println!("Event: {}", event_name(&other));
        }
        _ => {}
    }

    Ok(true)
}

fn event_name(event: &RealtimeServerEvent) -> &'static str {
    match event {
        RealtimeServerEvent::ConnectionStarted { .. } => "ConnectionStarted",
        RealtimeServerEvent::ConnectionFailed { .. } => "ConnectionFailed",
        RealtimeServerEvent::ConnectionFinished { .. } => "ConnectionFinished",
        RealtimeServerEvent::SessionStarted { .. } => "SessionStarted",
        RealtimeServerEvent::SessionFinished { .. } => "SessionFinished",
        RealtimeServerEvent::SessionFailed { .. } => "SessionFailed",
        RealtimeServerEvent::UsageResponse { .. } => "UsageResponse",
        RealtimeServerEvent::ConfigUpdated { .. } => "ConfigUpdated",
        RealtimeServerEvent::TtsSentenceStart { .. } => "TtsSentenceStart",
        RealtimeServerEvent::TtsSentenceEnd { .. } => "TtsSentenceEnd",
        RealtimeServerEvent::TtsResponse { .. } => "TtsResponse",
        RealtimeServerEvent::TtsEnded { .. } => "TtsEnded",
        RealtimeServerEvent::AsrInfo { .. } => "AsrInfo",
        RealtimeServerEvent::AsrResponse { .. } => "AsrResponse",
        RealtimeServerEvent::AsrEnded { .. } => "AsrEnded",
        RealtimeServerEvent::ChatResponse { .. } => "ChatResponse",
        RealtimeServerEvent::ChatTextQueryConfirmed { .. } => "ChatTextQueryConfirmed",
        RealtimeServerEvent::ChatEnded { .. } => "ChatEnded",
        RealtimeServerEvent::ConversationCreated { .. } => "ConversationCreated",
        RealtimeServerEvent::ConversationUpdated { .. } => "ConversationUpdated",
        RealtimeServerEvent::ConversationRetrieved { .. } => "ConversationRetrieved",
        RealtimeServerEvent::ConversationTruncated { .. } => "ConversationTruncated",
        RealtimeServerEvent::ConversationDeleted { .. } => "ConversationDeleted",
        RealtimeServerEvent::DialogCommonError { .. } => "DialogCommonError",
        RealtimeServerEvent::Error { .. } => "Error",
        RealtimeServerEvent::RawJson { .. } => "RawJson",
        RealtimeServerEvent::RawAudio { .. } => "RawAudio",
        RealtimeServerEvent::Raw { .. } => "Raw",
    }
}

fn start_microphone(device_name: Option<&str>, tx: mpsc::Sender<Bytes>) -> Result<Stream> {
    let host = cpal::default_host();
    let device = select_input_device(&host, device_name)?;
    let device_label = device
        .name()
        .unwrap_or_else(|_| "unknown input device".to_string());
    let supported = device
        .default_input_config()
        .context("failed to read default input config")?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let channels = config.channels as usize;
    let source_rate = config.sample_rate.0;

    println!(
        "Using microphone: {device_label}; format={sample_format:?}; channels={channels}; sample_rate={source_rate}"
    );

    let chunker = Arc::new(Mutex::new(InputChunker::new(
        channels,
        source_rate,
        ARK_INPUT_SAMPLE_RATE,
        ((ARK_INPUT_SAMPLE_RATE * INPUT_CHUNK_MS) / 1000) as usize,
    )));

    let stream = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32>(&device, &config, chunker, tx, f32_to_f32)?,
        SampleFormat::I16 => build_input_stream::<i16>(&device, &config, chunker, tx, i16_to_f32)?,
        SampleFormat::U16 => build_input_stream::<u16>(&device, &config, chunker, tx, u16_to_f32)?,
        other => bail!("unsupported microphone sample format: {other:?}"),
    };

    stream.play().context("failed to start microphone stream")?;
    Ok(stream)
}

fn build_input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    chunker: Arc<Mutex<InputChunker>>,
    tx: mpsc::Sender<Bytes>,
    convert: fn(T) -> f32,
) -> Result<Stream>
where
    T: cpal::SizedSample + Copy + Send + 'static,
{
    let err_fn = |error| eprintln!("microphone stream error: {error}");
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if let Ok(mut chunker) = chunker.lock() {
                    chunker.push_interleaved(data, convert, &tx);
                }
            },
            err_fn,
            None,
        )
        .context("failed to build microphone stream")
}

struct InputChunker {
    channels: usize,
    source_rate: u32,
    target_rate: u32,
    source: VecDeque<f32>,
    position: f64,
    output: Vec<f32>,
    chunk_samples: usize,
}

impl InputChunker {
    fn new(channels: usize, source_rate: u32, target_rate: u32, chunk_samples: usize) -> Self {
        Self {
            channels: channels.max(1),
            source_rate,
            target_rate,
            source: VecDeque::new(),
            position: 0.0,
            output: Vec::with_capacity(chunk_samples * 2),
            chunk_samples,
        }
    }

    fn push_interleaved<T>(&mut self, data: &[T], convert: fn(T) -> f32, tx: &mpsc::Sender<Bytes>)
    where
        T: Copy,
    {
        for frame in data.chunks(self.channels) {
            if frame.is_empty() {
                continue;
            }
            let mono =
                frame.iter().map(|sample| convert(*sample)).sum::<f32>() / frame.len() as f32;
            self.source.push_back(mono.clamp(-1.0, 1.0));
        }
        self.drain_resampled(tx);
    }

    fn drain_resampled(&mut self, tx: &mpsc::Sender<Bytes>) {
        let ratio = self.source_rate as f64 / self.target_rate as f64;
        while self.position + 1.0 < self.source.len() as f64 {
            let index = self.position.floor() as usize;
            let frac = (self.position - index as f64) as f32;
            let a = self.source[index];
            let b = self.source[index + 1];
            self.output.push(a + (b - a) * frac);
            self.position += ratio;

            if self.output.len() >= self.chunk_samples {
                let bytes = pcm_s16le_from_f32(&self.output[..self.chunk_samples]);
                self.output.drain(..self.chunk_samples);
                let _ = tx.try_send(Bytes::from(bytes));
            }
        }

        let consumed = self.position.floor() as usize;
        for _ in 0..consumed {
            self.source.pop_front();
        }
        self.position -= consumed as f64;

        let max_source_samples = self.source_rate as usize * 2;
        if self.source.len() > max_source_samples {
            self.source.clear();
            self.position = 0.0;
        }
    }
}

struct AudioPlayback {
    _stream: Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    source_rate: u32,
    output_rate: u32,
}

impl AudioPlayback {
    fn start(device_name: Option<&str>, source_rate: u32) -> Result<Self> {
        let host = cpal::default_host();
        let device = select_output_device(&host, device_name)?;
        let device_label = device
            .name()
            .unwrap_or_else(|_| "unknown output device".to_string());
        let supported = device
            .default_output_config()
            .context("failed to read default output config")?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let channels = config.channels as usize;
        let output_rate = config.sample_rate.0;
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(output_rate as usize)));

        println!(
            "Using speaker: {device_label}; format={sample_format:?}; channels={channels}; sample_rate={output_rate}"
        );

        let stream = match sample_format {
            SampleFormat::F32 => {
                build_output_stream::<f32>(&device, &config, buffer.clone(), f32_from_f32)?
            }
            SampleFormat::I16 => {
                build_output_stream::<i16>(&device, &config, buffer.clone(), i16_from_f32)?
            }
            SampleFormat::U16 => {
                build_output_stream::<u16>(&device, &config, buffer.clone(), u16_from_f32)?
            }
            other => bail!("unsupported speaker sample format: {other:?}"),
        };

        stream.play().context("failed to start speaker stream")?;
        Ok(Self {
            _stream: stream,
            buffer,
            source_rate,
            output_rate,
        })
    }

    fn enqueue_pcm_s16le(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() % 2 != 0 {
            bail!(
                "received odd-length pcm_s16le TTS payload: {} bytes",
                bytes.len()
            );
        }

        let samples = bytes
            .chunks_exact(2)
            .map(|chunk| i16_to_f32(i16::from_le_bytes([chunk[0], chunk[1]])))
            .collect::<Vec<_>>();
        let samples = resample_mono(&samples, self.source_rate, self.output_rate);
        let mut buffer = self
            .buffer
            .lock()
            .map_err(|_| anyhow!("speaker playback buffer lock poisoned"))?;
        buffer.extend(samples);

        let max_buffered = self.output_rate as usize * 30;
        if buffer.len() > max_buffered {
            let drop_count = buffer.len() - max_buffered;
            buffer.drain(..drop_count);
        }
        Ok(())
    }

    fn clear(&self) {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.clear();
        }
    }
}

fn build_output_stream<T>(
    device: &Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    convert: fn(f32) -> T,
) -> Result<Stream>
where
    T: cpal::SizedSample + Send + 'static,
{
    let channels = config.channels as usize;
    let err_fn = |error| eprintln!("speaker stream error: {error}");
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                if let Ok(mut buffer) = buffer.lock() {
                    for frame in data.chunks_mut(channels) {
                        let sample = buffer.pop_front().unwrap_or(0.0);
                        for output in frame {
                            *output = convert(sample);
                        }
                    }
                } else {
                    for output in data {
                        *output = convert(0.0);
                    }
                }
            },
            err_fn,
            None,
        )
        .context("failed to build speaker stream")
}

fn resample_mono(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == target_rate {
        return samples.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let output_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    for index in 0..output_len {
        let position = index as f64 * ratio;
        let base = position.floor() as usize;
        if base + 1 >= samples.len() {
            break;
        }
        let frac = (position - base as f64) as f32;
        let a = samples[base];
        let b = samples[base + 1];
        output.push(a + (b - a) * frac);
    }

    output
}

fn select_input_device(host: &cpal::Host, requested: Option<&str>) -> Result<Device> {
    if let Some(requested) = requested {
        let requested_lower = requested.to_ascii_lowercase();
        for device in host
            .input_devices()
            .context("failed to enumerate input devices")?
        {
            let name = device.name().unwrap_or_default();
            if name.to_ascii_lowercase().contains(&requested_lower) {
                return Ok(device);
            }
        }
        bail!("no input device matched BERRYBUDDY_INPUT_DEVICE={requested}");
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device found"))
}

fn select_output_device(host: &cpal::Host, requested: Option<&str>) -> Result<Device> {
    if let Some(requested) = requested {
        let requested_lower = requested.to_ascii_lowercase();
        for device in host
            .output_devices()
            .context("failed to enumerate output devices")?
        {
            let name = device.name().unwrap_or_default();
            if name.to_ascii_lowercase().contains(&requested_lower) {
                return Ok(device);
            }
        }
        bail!("no output device matched BERRYBUDDY_OUTPUT_DEVICE={requested}");
    }

    host.default_output_device()
        .ok_or_else(|| anyhow!("no default output device found"))
}

fn list_audio_devices() -> Result<()> {
    let host = cpal::default_host();
    println!("Input devices:");
    for device in host
        .input_devices()
        .context("failed to enumerate input devices")?
    {
        println!(
            "  {}",
            device.name().unwrap_or_else(|_| "unknown".to_string())
        );
    }
    println!("Output devices:");
    for device in host
        .output_devices()
        .context("failed to enumerate output devices")?
    {
        println!(
            "  {}",
            device.name().unwrap_or_else(|_| "unknown".to_string())
        );
    }
    Ok(())
}

fn pcm_s16le_from_f32(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&i16_from_f32(*sample).to_le_bytes());
    }
    bytes
}

fn f32_to_f32(sample: f32) -> f32 {
    sample.clamp(-1.0, 1.0)
}

fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

fn u16_to_f32(sample: u16) -> f32 {
    (sample as f32 / u16::MAX as f32) * 2.0 - 1.0
}

fn f32_from_f32(sample: f32) -> f32 {
    sample.clamp(-1.0, 1.0)
}

fn i16_from_f32(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn u16_from_f32(sample: f32) -> u16 {
    (((sample.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32) as u16
}

fn env_first(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| env_optional(name))
}

fn env_optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_default(name: &str, default: &str) -> String {
    env_optional(name).unwrap_or_else(|| default.to_string())
}

fn env_parse<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env_optional(name) {
        Some(value) => value
            .parse::<T>()
            .with_context(|| format!("failed to parse environment variable {name}={value}")),
        None => Ok(default),
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool> {
    match env_optional(name) {
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => bail!("failed to parse environment variable {name}={value} as bool"),
        },
        None => Ok(default),
    }
}

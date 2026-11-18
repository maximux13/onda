use std::f32::consts::PI;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use objc2::AnyThread;
use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap, CATapDescription,
    CATapMuteBehavior, kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceMainSubDeviceKey,
    kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioSubDeviceUIDKey, kAudioSubTapDriftCompensationKey,
    kAudioSubTapUIDKey,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFString, CFType};
use objc2_foundation::{NSArray, NSNumber, NSUUID};

use crate::state::EQ_FREQUENCIES;

const SAMPLE_RATE: f32 = 48_000.0;
const BIQUAD_Q: f32 = 1.414;

/// Errors returned when creating a Core Audio process tap.
#[derive(Debug, thiserror::Error)]
pub enum TapError {
    #[error("no process object IDs provided")]
    EmptyProcessIds,
    #[error("AudioHardwareCreateProcessTap failed with OSStatus {0}")]
    CreateProcessTap(i32),
    #[error("AudioHardwareCreateAggregateDevice failed with OSStatus {0}")]
    CreateAggregateDevice(i32),
    #[error("AudioDeviceCreateIOProcID failed with OSStatus {0}")]
    CreateIoProcId(i32),
    #[error("AudioDeviceStart failed with OSStatus {0}")]
    StartDevice(i32),
}

#[derive(Clone, Copy)]
struct BiquadBand {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
    last_gain_db: f32,
    active: bool,
}

impl BiquadBand {
    const fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
            last_gain_db: 0.0,
            active: false,
        }
    }

    #[inline]
    fn update_peaking(&mut self, f0: f32, gain_db: f32) {
        self.last_gain_db = gain_db;
        if gain_db.abs() < 0.05 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            self.active = false;
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / SAMPLE_RATE;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * BIQUAD_Q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        let inv_a0 = 1.0 / a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
        self.active = true;
    }

    #[inline]
    fn process(&mut self, x: f32, ch: usize) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
}

struct TapContext {
    gain: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    eq_enabled: Arc<AtomicBool>,
    eq_gains: Arc<[AtomicU32; 10]>,
    bands: [BiquadBand; 10],
}

/// Handle to an active Core Audio process tap, aggregate device, and IOProc.
pub struct AppTapHandle {
    gain: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    eq_enabled: Arc<AtomicBool>,
    eq_gains: Arc<[AtomicU32; 10]>,
    tap_id: u32,
    aggregate_device_id: u32,
    io_proc_id: AudioDeviceIOProcID,
    pub output_device_uid: String,
    ctx_ptr: *mut TapContext,
}

// SAFETY: All shared fields in `AppTapHandle` are `Arc<Atomic*>` or owned `String`/IDs;
// `ctx_ptr` is only dereferenced by the Core Audio IOProc while active and freed in `Drop`
// after stopping and destroying the IOProc.
unsafe impl Send for AppTapHandle {}
// SAFETY: `AppTapHandle` methods only perform atomic operations or read immutable fields.
unsafe impl Sync for AppTapHandle {}

impl AppTapHandle {
    /// Atomically updates the linear gain applied by the real-time audio callback.
    pub fn set_volume(&self, volume: f32) {
        self.gain
            .store(volume.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    /// Atomically updates the mute state applied by the real-time audio callback.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    /// Atomically updates the 10-band equalizer enabled state and per-band dB gains.
    pub fn set_eq(&self, enabled: bool, gains_db: &[f32; 10]) {
        self.eq_enabled.store(enabled, Ordering::Relaxed);
        for (slot, &g) in self.eq_gains.iter().zip(gains_db.iter()) {
            slot.store(g.clamp(-12.0, 12.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// Reads the latest real-time peak level `[0.0, 1.0]` and applies a slight decay.
    pub fn peak_level(&self) -> f32 {
        let p = f32::from_bits(self.peak.load(Ordering::Relaxed));
        self.peak.store((p * 0.85).to_bits(), Ordering::Relaxed);
        p.clamp(0.0, 1.0)
    }
}

impl Drop for AppTapHandle {
    fn drop(&mut self) {
        // SAFETY: Stopping and destroying the IOProc before freeing `ctx_ptr` ensures
        // Core Audio will not invoke `tap_io_proc` with a dangling `TapContext` pointer.
        unsafe {
            let _ = AudioDeviceStop(self.aggregate_device_id, self.io_proc_id);
            let _ = AudioDeviceDestroyIOProcID(self.aggregate_device_id, self.io_proc_id);
            let _ = AudioHardwareDestroyAggregateDevice(self.aggregate_device_id);
            let _ = AudioHardwareDestroyProcessTap(self.tap_id);
            if !self.ctx_ptr.is_null() {
                drop(Box::from_raw(self.ctx_ptr));
                self.ctx_ptr = std::ptr::null_mut();
            }
        }
    }
}

/// Real-time safe IOProc callback: scales float32 PCM samples from `in_input_data`
/// into `out_output_data`, applies the 10-band biquad peaking EQ when active,
/// and tracks peak level with zero allocations and zero locks.
unsafe extern "C-unwind" fn tap_io_proc(
    _in_device: u32,
    _in_now: NonNull<AudioTimeStamp>,
    in_input_data: NonNull<AudioBufferList>,
    _in_input_time: NonNull<AudioTimeStamp>,
    mut out_output_data: NonNull<AudioBufferList>,
    _in_output_time: NonNull<AudioTimeStamp>,
    in_client_data: *mut c_void,
) -> i32 {
    if in_client_data.is_null() {
        return 0;
    }

    // SAFETY: `in_client_data` was created via `Box::into_raw(Box::new(TapContext { ... }))`
    // and remains valid until `AppTapHandle::drop` after the IOProc is destroyed.
    let ctx = unsafe { &mut *(in_client_data as *mut TapContext) };
    let scale = if ctx.muted.load(Ordering::Relaxed) {
        0.0f32
    } else {
        f32::from_bits(ctx.gain.load(Ordering::Relaxed))
    };

    let eq_enabled = ctx.eq_enabled.load(Ordering::Relaxed);
    let mut use_eq = false;
    if eq_enabled {
        for (b, slot) in ctx.eq_gains.iter().enumerate() {
            let gain_db = f32::from_bits(slot.load(Ordering::Relaxed)).clamp(-12.0, 12.0);
            if (gain_db - ctx.bands[b].last_gain_db).abs() > 1e-4 {
                ctx.bands[b].update_peaking(EQ_FREQUENCIES[b], gain_db);
            }
            if ctx.bands[b].active {
                use_eq = true;
            }
        }
    }

    // SAFETY: Core Audio passes valid `NonNull<AudioBufferList>` pointers for the duration
    // of the IOProc callback, and `mNumberBuffers` matches the trailing `mBuffers` array.
    let in_list = unsafe { in_input_data.as_ref() };
    let out_list = unsafe { out_output_data.as_mut() };

    let in_count = in_list.mNumberBuffers as usize;
    let out_count = out_list.mNumberBuffers as usize;

    let in_buffers = unsafe { std::slice::from_raw_parts(in_list.mBuffers.as_ptr(), in_count) };
    let out_buffers =
        unsafe { std::slice::from_raw_parts_mut(out_list.mBuffers.as_mut_ptr(), out_count) };

    let mut max_abs = 0.0f32;

    for (i, out_buf) in out_buffers.iter_mut().enumerate() {
        if out_buf.mData.is_null() || out_buf.mDataByteSize == 0 {
            continue;
        }

        let n_channels = (out_buf.mNumberChannels as usize).max(1);
        let out_samples_len = (out_buf.mDataByteSize as usize) / std::mem::size_of::<f32>();
        // SAFETY: `out_buf.mData` is non-null and `mDataByteSize` is its valid byte length.
        let out_samples =
            unsafe { std::slice::from_raw_parts_mut(out_buf.mData as *mut f32, out_samples_len) };

        if let Some(in_buf) = in_buffers.get(i)
            && !in_buf.mData.is_null()
            && in_buf.mDataByteSize > 0
        {
            let in_samples_len = (in_buf.mDataByteSize as usize) / std::mem::size_of::<f32>();
            let count = out_samples_len.min(in_samples_len);
            // SAFETY: `in_buf.mData` is non-null and has at least `count` `f32` samples.
            let in_samples =
                unsafe { std::slice::from_raw_parts(in_buf.mData as *const f32, count) };

            if scale == 0.0 {
                out_samples[..count].fill(0.0);
            } else if !use_eq {
                for (dst, &src) in out_samples[..count].iter_mut().zip(in_samples.iter()) {
                    let s = src * scale;
                    if s.abs() > max_abs {
                        max_abs = s.abs();
                    }
                    *dst = s;
                }
            } else {
                for (idx, (dst, &src)) in out_samples[..count]
                    .iter_mut()
                    .zip(in_samples.iter())
                    .enumerate()
                {
                    let ch = if n_channels == 1 {
                        i.min(1)
                    } else {
                        (idx % n_channels).min(1)
                    };
                    let mut s = src * scale;
                    for band in &mut ctx.bands {
                        if band.active {
                            s = band.process(s, ch);
                        }
                    }
                    if !s.is_finite() {
                        s = 0.0;
                    }
                    if s.abs() > max_abs {
                        max_abs = s.abs();
                    }
                    *dst = s;
                }
            }
            if count < out_samples_len {
                out_samples[count..].fill(0.0);
            }
            continue;
        }

        out_samples.fill(0.0);
    }

    let prev = f32::from_bits(ctx.peak.load(Ordering::Relaxed));
    let next = max_abs.min(1.0).max(prev * 0.85);
    ctx.peak.store(next.to_bits(), Ordering::Relaxed);

    0
}

fn cstr_to_cfstring(cstr: &std::ffi::CStr) -> objc2_core_foundation::CFRetained<CFString> {
    CFString::from_str(cstr.to_str().expect("CoreAudio key must be valid UTF-8"))
}

/// Creates a Core Audio process tap for `process_object_ids`, wraps it in a private
/// aggregate device routed to `output_device_uid`, registers a real-time IOProc
/// for volume scaling and 10-band EQ, and starts audio IO.
pub fn create_process_tap(
    process_object_ids: &[u32],
    output_device_uid: &str,
    volume: f32,
    muted: bool,
    eq_enabled: bool,
    eq_gains: &[f32; 10],
) -> Result<AppTapHandle, TapError> {
    if process_object_ids.is_empty() {
        return Err(TapError::EmptyProcessIds);
    }
    let proc_nums: Vec<objc2::rc::Retained<NSNumber>> = process_object_ids
        .iter()
        .map(|&id| NSNumber::numberWithUnsignedInt(id))
        .collect();
    let proc_refs: Vec<&NSNumber> = proc_nums.iter().map(|n| &**n).collect();
    let procs = NSArray::from_slice(&proc_refs);

    let tap_desc = unsafe {
        CATapDescription::initStereoMixdownOfProcesses(CATapDescription::alloc(), &procs)
    };
    unsafe {
        tap_desc.setMuteBehavior(CATapMuteBehavior::Muted);
        tap_desc.setPrivate(true);
    }

    let tap_uuid = unsafe { tap_desc.UUID().UUIDString() }.to_string();

    let mut tap_id: u32 = 0;
    let status = unsafe { AudioHardwareCreateProcessTap(Some(&tap_desc), &mut tap_id) };
    if status != 0 {
        return Err(TapError::CreateProcessTap(status));
    }

    let agg_uid = NSUUID::UUID().UUIDString().to_string();
    let agg_name = format!("VolumeMixer-Tap-{}", process_object_ids[0]);

    // Sub-device list containing the target output device
    let sub_device_uid_key = cstr_to_cfstring(kAudioSubDeviceUIDKey);
    let sub_device_uid_val = CFString::from_str(output_device_uid);
    let sub_device_dict = CFDictionary::<CFType, CFType>::from_slices(
        &[sub_device_uid_key.as_ref()],
        &[sub_device_uid_val.as_ref()],
    );
    let sub_device_list = CFArray::<CFType>::from_objects(&[sub_device_dict.as_ref()]);

    // Tap list containing the created process tap
    let sub_tap_uid_key = cstr_to_cfstring(kAudioSubTapUIDKey);
    let sub_tap_uid_val = CFString::from_str(&tap_uuid);
    let sub_tap_drift_key = cstr_to_cfstring(kAudioSubTapDriftCompensationKey);
    let sub_tap_dict = CFDictionary::<CFType, CFType>::from_slices(
        &[sub_tap_uid_key.as_ref(), sub_tap_drift_key.as_ref()],
        &[sub_tap_uid_val.as_ref(), CFBoolean::new(true).as_ref()],
    );
    let tap_list = CFArray::<CFType>::from_objects(&[sub_tap_dict.as_ref()]);

    // Aggregate device description dictionary
    let k_name = cstr_to_cfstring(kAudioAggregateDeviceNameKey);
    let k_uid = cstr_to_cfstring(kAudioAggregateDeviceUIDKey);
    let k_main = cstr_to_cfstring(kAudioAggregateDeviceMainSubDeviceKey);
    let k_sub_devices = cstr_to_cfstring(kAudioAggregateDeviceSubDeviceListKey);
    let k_taps = cstr_to_cfstring(kAudioAggregateDeviceTapListKey);
    let k_tap_auto_start = cstr_to_cfstring(kAudioAggregateDeviceTapAutoStartKey);
    let k_is_private = cstr_to_cfstring(kAudioAggregateDeviceIsPrivateKey);

    let v_name = CFString::from_str(&agg_name);
    let v_uid = CFString::from_str(&agg_uid);
    let v_main = CFString::from_str(output_device_uid);
    let v_true = CFBoolean::new(true);

    let agg_dict = CFDictionary::<CFType, CFType>::from_slices(
        &[
            k_name.as_ref(),
            k_uid.as_ref(),
            k_main.as_ref(),
            k_sub_devices.as_ref(),
            k_taps.as_ref(),
            k_tap_auto_start.as_ref(),
            k_is_private.as_ref(),
        ],
        &[
            v_name.as_ref(),
            v_uid.as_ref(),
            v_main.as_ref(),
            sub_device_list.as_ref(),
            tap_list.as_ref(),
            v_true.as_ref(),
            v_true.as_ref(),
        ],
    );

    let mut aggregate_device_id: u32 = 0;
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(
            agg_dict.as_opaque(),
            NonNull::from(&mut aggregate_device_id),
        )
    };
    if status != 0 {
        unsafe {
            let _ = AudioHardwareDestroyProcessTap(tap_id);
        }
        return Err(TapError::CreateAggregateDevice(status));
    }

    let gain = Arc::new(AtomicU32::new(volume.clamp(0.0, 2.0).to_bits()));
    let muted_flag = Arc::new(AtomicBool::new(muted));
    let peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
    let eq_enabled_flag = Arc::new(AtomicBool::new(eq_enabled));
    let eq_gains_atomic = Arc::new(std::array::from_fn(|i| {
        AtomicU32::new(eq_gains[i].clamp(-12.0, 12.0).to_bits())
    }));
    let mut bands = [BiquadBand::new(); 10];
    for (b, &g) in eq_gains.iter().enumerate() {
        bands[b].update_peaking(EQ_FREQUENCIES[b], g.clamp(-12.0, 12.0));
    }

    let ctx_ptr = Box::into_raw(Box::new(TapContext {
        gain: Arc::clone(&gain),
        muted: Arc::clone(&muted_flag),
        peak: Arc::clone(&peak),
        eq_enabled: Arc::clone(&eq_enabled_flag),
        eq_gains: Arc::clone(&eq_gains_atomic),
        bands,
    }));

    let mut io_proc_id: AudioDeviceIOProcID = None;
    let status = unsafe {
        AudioDeviceCreateIOProcID(
            aggregate_device_id,
            Some(tap_io_proc),
            ctx_ptr.cast::<c_void>(),
            NonNull::from(&mut io_proc_id),
        )
    };
    if status != 0 {
        unsafe {
            let _ = AudioHardwareDestroyAggregateDevice(aggregate_device_id);
            let _ = AudioHardwareDestroyProcessTap(tap_id);
            drop(Box::from_raw(ctx_ptr));
        }
        return Err(TapError::CreateIoProcId(status));
    }

    let status = unsafe { AudioDeviceStart(aggregate_device_id, io_proc_id) };
    if status != 0 {
        unsafe {
            let _ = AudioDeviceDestroyIOProcID(aggregate_device_id, io_proc_id);
            let _ = AudioHardwareDestroyAggregateDevice(aggregate_device_id);
            let _ = AudioHardwareDestroyProcessTap(tap_id);
            drop(Box::from_raw(ctx_ptr));
        }
        return Err(TapError::StartDevice(status));
    }

    Ok(AppTapHandle {
        gain,
        muted: muted_flag,
        peak,
        eq_enabled: eq_enabled_flag,
        eq_gains: eq_gains_atomic,
        tap_id,
        aggregate_device_id,
        io_proc_id,
        output_device_uid: output_device_uid.to_string(),
        ctx_ptr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_flat_is_identity() {
        let mut band = BiquadBand::new();
        band.update_peaking(1000.0, 0.0);

        for i in 0..32 {
            let input = ((i as f32) * 0.2).sin();
            let output = band.process(input, 0);
            assert!((output - input).abs() < 1e-6);
        }
    }

    #[test]
    fn test_biquad_boost_and_cut_modify_signal() {
        let mut boost_band = BiquadBand::new();
        boost_band.update_peaking(1000.0, 6.0);

        let mut cut_band = BiquadBand::new();
        cut_band.update_peaking(1000.0, -6.0);

        let mut boost_energy = 0.0f32;
        let mut cut_energy = 0.0f32;

        // 1 kHz sine wave at 48 kHz sample rate
        for n in 0..480 {
            let sample = (2.0 * std::f32::consts::PI * 1000.0 * (n as f32) / 48_000.0).sin();
            let b_out = boost_band.process(sample, 0);
            let c_out = cut_band.process(sample, 0);
            if n > 48 {
                boost_energy += b_out * b_out;
                cut_energy += c_out * c_out;
            }
        }

        assert!(boost_energy > cut_energy * 2.0);
    }
}

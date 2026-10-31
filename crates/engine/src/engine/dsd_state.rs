//! State and buffers for native DSD bitstream and DoP transport.

use crate::buffer::DsdByteBuffer;
use crate::decode::dsd::DsdWireFormat;
use crate::decode::DsdTransportReport;
use crate::dsp::sdm::{SdmOutput, SdmRatePlan};
use std::sync::Arc;

/// Build the reason text for why DoP cannot engage because the output is not
/// exclusive (used by the `load_track` fallback warning). When the user is on
/// the Auto/shared backend it names a concrete exclusive backend to switch to,
/// so the failure is actionable rather than a dead-end log line.
pub fn dop_exclusive_reason(
    out_info: &crate::output::OutputInfo,
    requested_backend: config::AudioBackend,
) -> String {
    if out_info.is_fallback {
        match &out_info.fallback_reason {
            Some(r) => format!("the exclusive backend request fell back to a shared device ({r})"),
            None => "the exclusive backend request fell back to a shared device".to_string(),
        }
    } else if requested_backend == config::AudioBackend::Auto {
        let exclusive_names = if cfg!(target_os = "linux") {
            "ExclusiveAlsa (or select a direct hw: device)".to_string()
        } else if cfg!(target_os = "windows") {
            "ExclusiveAsio (WASAPI exclusive requires a native IAudioClient \
             backend not available through cpal)"
                .to_string()
        } else {
            "ExclusiveCoreAudioHog (a native CoreAudio hog-mode backend, \
             not available through cpal)"
                .to_string()
        };
        format!(
            "the backend is Auto (shared device); switch to an exclusive backend \
             ({exclusive_names})"
        )
    } else if requested_backend == config::AudioBackend::ExclusiveAsio
        && !cfg!(feature = "asio")
        && !cfg!(feature = "asio-native")
    {
        "the ASIO backend is not compiled in (enable the 'asio-native' or 'asio' feature)"
            .to_string()
    } else {
        "the selected backend did not provide exclusive access".to_string()
    }
}

/// The one transport the engine will attempt for a newly opened source.
///
/// Native DSD, DoP and PCM→SDM all reconfigure the same output stream, so
/// they must be mutually exclusive. Deriving the choice once from the source,
/// the DSD output preference and the SDM config keeps negotiation from
/// engaging two transports at once (the previous code could negotiate native
/// DSD and then reconfigure the device for SDM on the same load).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DsdTransportPlan {
    /// PCM (or DSD→PCM conversion) through the normal DSP chain.
    Pcm,
    /// Raw 1-bit DSD bitstream to a native-DSD DAC.
    Native,
    /// DSD-over-PCM packed into 24-bit frames.
    Dop,
    /// PCM→SDM modulation to native DSD (also used to re-modulate DSD
    /// sources when `sdm.dsd_bypass` is off).
    Sdm,
}

/// Choose the transport plan for a source.
///
/// Precedence:
/// 1. A PCM source uses SDM when enabled, otherwise PCM.
/// 2. A DSD source re-modulates through SDM when SDM is enabled and
///    `dsd_bypass` is off. This deliberately overrides `dsd_output`: the user
///    asked for the modulator, and letting native/DoP run first would leave
///    the device in the wrong mode for the SDM attempt.
/// 3. Otherwise a DSD source follows `dsd_output` exactly.
pub(crate) fn plan_transport(
    is_dsd: bool,
    dsd_output: config::DsdOutput,
    sdm: &config::SdmConfig,
) -> DsdTransportPlan {
    if is_dsd {
        if sdm.enabled && !sdm.dsd_bypass {
            return DsdTransportPlan::Sdm;
        }
    } else if sdm.enabled {
        return DsdTransportPlan::Sdm;
    }

    match dsd_output {
        config::DsdOutput::PcmConvert => DsdTransportPlan::Pcm,
        config::DsdOutput::DoP => DsdTransportPlan::Dop,
        config::DsdOutput::NativeDsd => {
            if is_dsd {
                DsdTransportPlan::Native
            } else {
                DsdTransportPlan::Pcm
            }
        }
    }
}

pub(crate) struct DsdTransportState {
    /// True when the current track is being output as DSD-over-PCM (DoP): raw
    /// DSD packed into 24-bit frames, DSP fully bypassed, no resampling.
    pub(crate) dop_active: bool,
    /// Output rate of the active DoP stream (bit_rate / 16); 0 when inactive.
    pub(crate) dop_rate: u32,
    /// True when the current track is being output as native DSD: raw 1-bit
    /// bitstream to a DSD-capable DAC, entire f32 DSP path structurally
    /// bypassed (no decimation, no resampling, no filters).
    pub(crate) native_dsd_active: bool,
    /// Negotiated native-DSD wire format (e.g. DSD_U8); `None` when native
    /// DSD is not active.
    pub(crate) dsd_wire_format: Option<DsdWireFormat>,
    /// Byte ring the engine's native-DSD path pushes raw interleaved DSD
    /// bytes into; the DSD-capable output backend drains it to the DAC.
    pub(crate) dsd_byte_buffer: Option<Arc<DsdByteBuffer>>,
    /// Reusable packed-wire scratch buffer for the native-DSD path
    /// (allocated once; the hot path stays allocation-free).
    pub(crate) dsd_pack_scratch: Vec<u8>,
    /// Explicit DSD transport negotiation report (§7): requested vs actual
    /// transport plus ordered fallback steps (native → DoP → PCM).
    pub(crate) dsd_transport_report: DsdTransportReport,

    /// True while a PCM source is being converted to native DSD by the
    /// engine's own modulator. Distinct from `native_dsd_active`: the DSP
    /// chain still runs at the working rate, and the transport is native DSD.
    pub(crate) sdm_active: bool,
    /// The per-channel interpolation + modulation + packing stage. `None`
    /// when PCM output is active.
    pub(crate) sdm_output: Option<SdmOutput>,
    /// Negotiated PCM→SDM rate plan (working rate, DSD rate, factor).
    pub(crate) sdm_plan: Option<SdmRatePlan>,
    /// Why a requested PCM→SDM conversion did not engage. Preserved across
    /// the whole track so the UI can explain the PCM fallback.
    pub(crate) sdm_fallback_reason: Option<String>,
}

impl Default for DsdTransportState {
    fn default() -> Self {
        Self {
            dop_active: false,
            dop_rate: 0,
            native_dsd_active: false,
            dsd_wire_format: None,
            dsd_byte_buffer: None,
            dsd_pack_scratch: Vec::with_capacity(1 << 16),
            dsd_transport_report: DsdTransportReport::default(),
            sdm_active: false,
            sdm_output: None,
            sdm_plan: None,
            sdm_fallback_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdm(enabled: bool, dsd_bypass: bool) -> config::SdmConfig {
        config::SdmConfig {
            enabled,
            dsd_bypass,
            ..Default::default()
        }
    }

    #[test]
    fn pcm_source_only_uses_sdm_when_enabled() {
        assert_eq!(
            plan_transport(false, config::DsdOutput::PcmConvert, &sdm(false, true)),
            DsdTransportPlan::Pcm
        );
        assert_eq!(
            plan_transport(false, config::DsdOutput::PcmConvert, &sdm(true, true)),
            DsdTransportPlan::Sdm
        );
        // dsd_output selects the transport for DSD sources only; a PCM source
        // must not turn it into a DSD transport.
        assert_eq!(
            plan_transport(false, config::DsdOutput::NativeDsd, &sdm(false, true)),
            DsdTransportPlan::Pcm
        );
    }

    #[test]
    fn dsd_source_follows_dsd_output_when_bypassing_sdm() {
        for (output, expected) in [
            (config::DsdOutput::PcmConvert, DsdTransportPlan::Pcm),
            (config::DsdOutput::DoP, DsdTransportPlan::Dop),
            (config::DsdOutput::NativeDsd, DsdTransportPlan::Native),
        ] {
            assert_eq!(
                plan_transport(true, output, &sdm(false, true)),
                expected,
                "disabled SDM must not change the requested DSD transport"
            );
            assert_eq!(
                plan_transport(true, output, &sdm(true, true)),
                expected,
                "dsd_bypass=true must keep native DSD bit-perfect"
            );
        }
    }

    #[test]
    fn dsd_source_remodulates_through_sdm_when_bypass_is_off() {
        for output in [
            config::DsdOutput::PcmConvert,
            config::DsdOutput::DoP,
            config::DsdOutput::NativeDsd,
        ] {
            assert_eq!(
                plan_transport(true, output, &sdm(true, false)),
                DsdTransportPlan::Sdm,
                "SDM with dsd_bypass=false must own the transport for DSD sources"
            );
        }
    }

    #[test]
    fn bypass_flag_is_inert_while_sdm_is_disabled() {
        for output in [
            config::DsdOutput::PcmConvert,
            config::DsdOutput::DoP,
            config::DsdOutput::NativeDsd,
        ] {
            assert_eq!(
                plan_transport(true, output, &sdm(false, false)),
                plan_transport(true, output, &sdm(false, true))
            );
        }
    }
}

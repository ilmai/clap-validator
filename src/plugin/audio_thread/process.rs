//! Data structures and functions surrounding audio processing.

use anyhow::Result;
use clap_sys::events::{
    clap_event_header, clap_event_transport, CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_TRANSPORT,
    CLAP_TRANSPORT_HAS_BEATS_TIMELINE, CLAP_TRANSPORT_HAS_SECONDS_TIMELINE,
    CLAP_TRANSPORT_HAS_TEMPO, CLAP_TRANSPORT_HAS_TIME_SIGNATURE, CLAP_TRANSPORT_IS_PLAYING,
};
use clap_sys::fixedpoint::{CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR};

/// The input and output data for a call to `clap_plugin::process()`.
pub struct ProcessData<'a> {
    /// The input and output audio buffers.
    pub buffers: AudioBuffers<'a>,
    /// The current transport information. This is populated when constructing this object, and the
    /// transport can be advanced `N` samples using the
    /// [`advance_transport()`][Self::advance_transport()] method.
    transport_info: clap_event_transport,
    /// The current sample position. This is used to recompute values in `transport_info`.
    sample_pos: u32,
    /// The current sample rate.
    sample_rate: f64,
    // TODO: Events
    // TODO: Maybe do something with `steady_time`
}

/// Audio buffers for [`ProcessData`]. CLAP allows hosts to do both in-place and out-of-place
/// processing, so we'll support and test both methods.
pub enum AudioBuffers<'a> {
    /// Out-of-place processing with separate non-aliasing input and output buffers.
    OutOfPlace(OutOfPlaceAudioBuffers<'a>),
    // TODO: In-place processing, figure out a safe abstraction for this if the in-place pairs
    //       aren't symmetrical between the inputs and outputs (e.g. when it's not just
    //       input1<->output1, input2<->output2, etc.).
}

/// Audio buffers for out-of-place processing. This wrapper allocates and sets up the channel
/// pointers. To avoid an unnecessary level of abstraction where the `Vec<Vec<f32>>`s need to be
/// converted to a slice of slices, this data structure borrows the vectors directly.
//
// TODO: This only does f32 for now, we'll also want to test f64 and mixed configurations later.
pub struct OutOfPlaceAudioBuffers<'a> {
    // These are all indexed by `[port_idx][channel_idx][sample_idx]`
    inputs: &'a [Vec<Vec<f32>>],
    outputs: &'a mut [Vec<Vec<f32>>],
    input_channel_pointers: Vec<Vec<*const f32>>,
    output_channel_pointers: Vec<Vec<*const f32>>,

    /// The number of samples for this buffer. This is consistent across all inner vectors.
    num_samples: usize,
}

impl<'a> ProcessData<'a> {
    /// Initialize the process data using the given audio buffers. The transport information will be
    /// initialized at the start of the project, and it can be moved using the
    /// [`advance_transport()`][Self::advance_transport()] method.
    //
    // TODO: More transport info options. Missing fields, loop regions, flags, etc.
    pub fn new(
        buffers: AudioBuffers<'a>,
        sample_rate: f64,
        tempo: f32,
        time_sig_numerator: u16,
        time_sig_denominator: u16,
    ) -> Self {
        ProcessData {
            buffers,
            transport_info: clap_event_transport {
                header: clap_event_header {
                    size: std::mem::size_of::<clap_event_transport>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_TRANSPORT,
                    flags: 0,
                },
                flags: CLAP_TRANSPORT_HAS_TEMPO
                    | CLAP_TRANSPORT_HAS_BEATS_TIMELINE
                    | CLAP_TRANSPORT_HAS_SECONDS_TIMELINE
                    | CLAP_TRANSPORT_HAS_TIME_SIGNATURE
                    | CLAP_TRANSPORT_IS_PLAYING,
                song_pos_beats: 0,
                song_pos_seconds: 0,
                tempo: tempo as f64,
                tempo_inc: 0.0,
                // These four currently aren't used
                loop_start_beats: 0,
                loop_end_beats: 0,
                loop_start_seconds: 0,
                loop_end_seconds: 0,
                bar_start: 0,
                bar_number: 0,
                tsig_num: time_sig_numerator,
                tsig_denom: time_sig_denominator,
            },
            sample_pos: 0,
            sample_rate,
        }
    }

    /// Get current the transport information.
    pub fn transport_info(&self) -> clap_event_transport {
        self.transport_info
    }

    /// Advance the transport by a certain number of samples
    pub fn advance_transport(&mut self, samples: u32) {
        self.sample_pos += samples;

        self.transport_info.song_pos_beats = ((self.sample_pos as f64 / self.sample_rate / 60.0
            * self.transport_info.tempo)
            * CLAP_BEATTIME_FACTOR as f64)
            .round() as i64;
        self.transport_info.song_pos_seconds = ((self.sample_pos as f64 / self.sample_rate)
            * CLAP_SECTIME_FACTOR as f64)
            .round() as i64;
    }
}

impl<'a> OutOfPlaceAudioBuffers<'a> {
    /// Construct the out of place audio buffers. This allocates the channel pointers that are
    /// handed to the plugin in the process function. The function will return an error if the
    /// sample count doesn't match between all input and outputs vectors.
    pub fn new(inputs: &'a [Vec<Vec<f32>>], outputs: &'a mut [Vec<Vec<f32>>]) -> Result<Self> {
        // We need to make sure all inputs and outputs have the same number of channels. Since zero
        // channel ports are technically legal and it's also possible to not have any inputs we
        // can't just start with the first input.
        let mut num_samples = None;
        for channel_slices in inputs.iter().chain(outputs.iter()) {
            for channel_slice in channel_slices {
                match num_samples {
                    Some(num_samples) if channel_slice.len() != num_samples => anyhow::bail!(
                        "Inconsistent sample counts in audio buffers. Expected {}, found {}.",
                        num_samples,
                        channel_slice.len()
                    ),
                    Some(_) => (),
                    None => num_samples = Some(channel_slice.len()),
                }
            }
        }

        let input_channel_pointers: Vec<Vec<*const f32>> = inputs
            .iter()
            .map(|channel_slices| {
                channel_slices
                    .iter()
                    .map(|channel_slice| channel_slice.as_ptr())
                    .collect()
            })
            .collect();
        // These are always `*const` pointers in CLAP, even for output buffers
        let output_channel_pointers: Vec<Vec<*const f32>> = outputs
            .iter()
            .map(|channel_slices| {
                channel_slices
                    .iter()
                    .map(|channel_slice| channel_slice.as_ptr())
                    .collect()
            })
            .collect();

        Ok(Self {
            inputs,
            outputs,
            input_channel_pointers,
            output_channel_pointers,

            num_samples: num_samples.unwrap_or(0),
        })
    }

    /// The number of samples in the buffer.
    pub fn len(&self) -> usize {
        self.num_samples
    }

    /// Pointers for the inputs. `buffer.input_channel_pointers()[port_idx].as_ptr()` can be used to
    /// populate `clap_audio_buffer::data32`.
    pub fn input_channel_pointers(&self) -> &[Vec<*const f32>] {
        &self.input_channel_pointers
    }

    /// Pointers for the outputs. `buffer.output_channel_pointers()[port_idx].as_ptr()` can be used
    /// to populate `clap_audio_buffer::data32`.
    pub fn output_channel_pointers(&self) -> &[Vec<*const f32>] {
        &self.input_channel_pointers
    }
}
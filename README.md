# rawrscope

rawrscope is a fast, user-friendly, and cross-platform tool for creating
oscilloscope visualizations of audio, typically chiptune.

## Build Status

*Note: rawrscope uses custom Cargo profiles for CI and thus requires a nightly
toolchain.*

![](https://github.com/chiptunecafe/rawrscope/workflows/CI/badge.svg)

*TODO: somehow have separate badges for each os*

## Features

\* = unimplemented

* \*Intuitive interface
* Realtime editor and preview
* Antialiased, GPU accelerated line rendering
* Many centering algorithms
  * \*Peak Speed
  * \*Fundamental Phase
  * \*Crosscorrelation
  * \*External Trigger
* \*High-quality trigger generator for external trigger mode
* Audio manipulation tools (\*trim, fade in/out)
* \*Node-based audio routing interface
  * Automatic master audio generation
  * Stereo upmixing/downmixing
* \*Visual templates and presets for a quicker workflow
* \*Built-in video export
* \*Arbitrary post-processing shaders
* \*Command line interface
* Written in [Rust](https://www.rust-lang.org) :)

## Installation

### From CI

View the CI runs [here](https://github.com/chiptunecafe/rawrscope/actions) (**must
be logged into GitHub to view**), select the latest successful run, and download
the appropriate ZIP file from the "Artifacts" tab in the top right. Then, just
extract the archive.

### From source

Clone the repository and run `cargo +nightly build -Z unstable-options --profile deploy`.

CMake and Python are required for building dependencies.

The deploy profile strips away all debuginformation and enables LTO, whereas the release
profile does not enable LTO and retains debugging symbols. The default dev profile
should only be used for testing if things will compile since the optimizations that are
applied in the other profiles are necessary for acceptable performance.

## Tutorial

*TODO*

## Contributing

Any help resolving issues is appreciated, issues tagged
[`X=help needed`](https://github.com/chiptunecafe/rawrscope/issues?q=label%3A%22X%3Dhelp+needed%22) 
are likely a good place to start. If coding isn't your thing, then issues tagged
[`X=feedback wanted`](https://github.com/chiptunecafe/rawrscope/issues?q=label%3A%22X%3Dfeedback+wanted%22)
could still use your help.

All code contributed should be formatted with `rustfmt` before being merged.

rawrscope is licenced under GPLv3+, see
[`COPYING`](https://github.com/chiptunecafe/rawrscope/blob/master/COPYING)
for details.

## Future Roadmap

* Full timeline editor for audio sources
* Integrated rendering of chiptune files
* Scripting support

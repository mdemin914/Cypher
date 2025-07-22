# Cypher Looper

**A feature-rich, open-source music looper with synthesizer, written entirely in Rust.**

Cypher is an integrated environment for music creation, designed for musicians, producers, and live performers. At its heart is a powerful 12-track stereo looper, but it extends far beyond that, offering a deep, dual-engine synthesizer, a wavetable engine, a multi-sample instrument engine, a one-shot pad sampler, and a unique music theory assistant to spark creativity.

Built with performance and stability in mind, Cypher leverages the power of Rust and a clean, immediate-mode GUI to provide a responsive and intuitive user experience.

<img width="3432" height="1386" alt="image" src="https://github.com/user-attachments/assets/47020498-bbb3-4255-b345-d1d1c19afea0" />

## Core Features

Cypher combines several powerful tools into a single, cohesive application:

*   **Looping Core:**
    *   **12 Independent Stereo Looper Tracks:** Record, play, and overdub layers with ease.
    *   **Automatic Tempo Sync:** The first loop sets the global BPM, with an optional "BPM Rounding" feature to lock to a perfect tempo.
    *   **Full Mixer Control:** Each looper track has its own mixer channel with dedicated controls for volume, mute, and solo.

*   **Dual-Engine Synthesis Powerhouse:**
    *   **Two Fully Independent Synth Engines:** Layer sounds or create complex splits. Each engine can be one of two types:
    *   **Wavetable Engine:** A sophisticated 4-oscillator wavetable synth with position morphing, layer mixing, and a unique "Bell" filter for spectral shaping. Load your own WAV files to create custom wavetables.
 <img width="1022" height="790" alt="image" src="https://github.com/user-attachments/assets/1b736ffb-6d8e-443a-b4d4-8f324f885b03" />

 
    *   **Multi-Sample Engine:** Build complex, key-mapped instruments with 8 sample slots per engine, each with its own root note.
 <img width="1016" height="822" alt="image" src="https://github.com/user-attachments/assets/e21d1a4a-243f-46c6-9128-70c71446e6b6" />

    *   **Deep Modulation:** Both engines feature a flexible modulation matrix, two feature-rich LFOs, and dedicated ADSR envelopes for both amplitude and the filter.
 <img width="1013" height="367" alt="image" src="https://github.com/user-attachments/assets/c0be915c-791c-46eb-9417-9137036202e4" />


    *   **Built-in state-variable filter (LP/HP/BP) and a configurable saturation module for adding warmth and character.

*   **Creative Tools:**
    *   **16-Pad Sampler:** A classic one-shot sampler perfect for drums and effects. Load samples via drag-and-drop and build custom kits.
 <img width="494" height="557" alt="image" src="https://github.com/user-attachments/assets/ca5d3b0b-0132-403e-9635-df8f644f9b9b" />

    *   **Music Theory Assistant (88Keys):** An integrated piano keyboard that can visualize 14 different musical scales or provide harmonically-aware chord suggestions based on what you play, using customizable "Chord Styles".
 <img width="3434" height="204" alt="image" src="https://github.com/user-attachments/assets/2953ac9d-562d-4a6c-b064-442d4f6f565b" />

    *   
    *   **Live Audio Input:** Process live audio from a microphone or instrument, route it into the loopers, or simply monitor it.
<img width="856" height="105" alt="image" src="https://github.com/user-attachments/assets/89755681-467d-4295-88b3-9524868742f6" />

*   **Extensible and Customizable:**
    *   **Fully Themeable UI:** Every color in the application is defined in a simple JSON file. A built-in theme editor allows you to create, save, and share your own visual styles.
 <img width="418" height="437" alt="image" src="https://github.com/user-attachments/assets/b3025364-1abf-4b5f-8473-472dbca1e970" />

    *   **Asset Library:** Cypher automatically scans a dedicated directory for your Samples, Synth Presets, Sampler Kits, and Themes, organizing them in a convenient browser.
<img width="1001" height="208" alt="image" src="https://github.com/user-attachments/assets/b59f2914-2a55-4f07-a5f7-e17cad00195c" />

    *   **Options Menu:** Cypher has its own low latency audio implementation, including jack for linux. 
<img width="559" height="451" alt="image" src="https://github.com/user-attachments/assets/ce53e278-3813-461f-80e4-7bf58164a4d9" />


## Technology Stack

This project is a showcase of modern, high-performance audio development in Rust.

*   **Language:** [**Rust**](https://www.rust-lang.org/)
*   **GUI Framework:** [**egui**](https://github.com/emilk/egui) - A pure-Rust, immediate-mode GUI library.
*   **Audio I/O:** [**cpal**](https://github.com/RustAudio/cpal) - A cross-platform audio I/O library.
*   **MIDI Input:** [**midir**](https://github.com/Boddlnagg/midir)
*   **Parallel Processing:** [**rayon**](https://github.com/rayon-rs/rayon) for high-performance voice processing in the synth engine.

## Contributing

Contributions are welcome and highly appreciated! Whether you're a musician, a developer, or a designer, there are many ways to help out:

*   **Code:** Submitting bug fixes, implementing new features, or refactoring for performance.
*   **Testing:** Finding and reporting bugs and providing feedback on usability.
*   **Design:** Creating new themes or designing synth presets and sampler kits to be bundled with the application.
*   **Documentation:** Improving this README, adding code comments, or writing user guides.

Please feel free to open an issue to discuss any ideas or report problems.

## License

This project is licensed under the [MIT License](LICENSE).

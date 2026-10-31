# Attribution

Third-party assets bundled with StarBooru.

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `apps/app/src/icons.rs` (SVG path data, most icons; `Search` changed to an open lens with a highlight, `DeleteForever` is `Delete` with an exclamation mark), `tools/vscode-starbooru/media/star.svg` | MingCute Icon (fill) | MingCute Design | https://github.com/Richard9394/MingCute | Apache-2.0 |
| `apps/app/src/icons.rs` (SVG path data: `Tune`, `ThumbUp`, `ThumbDown`) | Material Symbols Rounded | Google | https://github.com/google/material-design-icons | Apache-2.0 |
| `apps/app/src/icons.rs` (SVG path data: `Edit`) | Solar Bold | 480 Design | https://www.figma.com/community/file/1166831539721848736 | CC BY 4.0 |
| `apps/app/src/icons.rs` (`Upload`, redrawn in MingCute's style) | Layout from Fluent UI System Icons | Microsoft | https://github.com/microsoft/fluentui-system-icons | MIT |
| `apps/app/src/icons.rs` (`Translate`, redrawn with thicker strokes) | Letterforms from Material Symbols Rounded | Google | https://github.com/google/material-design-icons | Apache-2.0 |
| `vendor/tao` (Rust source, patched) | tao 0.34.8 with tao#1218 | Tauri Programme within The Commons Conservancy, the winit contributors | https://github.com/tauri-apps/tao | Apache-2.0 OR MIT |

Not bundled: downloaded from Hugging Face by each server when its admin installs them, each pinned to a commit in `crates/core/src/ai_files.rs`.

| Model | Author | Source | License |
|---|---|---|---|
| ML-Danbooru TResNet-D and CAFormer (ONNX) | 7eu7d7, deepghs | https://huggingface.co/deepghs/ml-danbooru-onnx | MIT |
| JoyTag | fancyfeast | https://huggingface.co/fancyfeast/joytag | Apache-2.0 |
| WD ViT Tagger v3 | SmilingWolf | https://huggingface.co/SmilingWolf/wd-vit-tagger-v3 | Apache-2.0 |
| WD ConvNext Tagger v3 | SmilingWolf | https://huggingface.co/SmilingWolf/wd-convnext-tagger-v3 | Apache-2.0 |
| WD SwinV2 Tagger v3 | SmilingWolf | https://huggingface.co/SmilingWolf/wd-swinv2-tagger-v3 | Apache-2.0 |
| WD ViT Large Tagger v3 | SmilingWolf | https://huggingface.co/SmilingWolf/wd-vit-large-tagger-v3 | Apache-2.0 |
| Danbooru Tagger v1 | Redstonexs | https://huggingface.co/Redstonexs/danbooru-tagger-v1 | Apache-2.0 |
| WD EVA02 Large Tagger v3 | SmilingWolf | https://huggingface.co/SmilingWolf/wd-eva02-large-tagger-v3 | Apache-2.0 |
| Opus-MT (ONNX) | Helsinki-NLP, Xenova | https://huggingface.co/Xenova/opus-mt-ja-en | Apache-2.0 |
| M2M-100 418M (ONNX) | Meta, Xenova | https://huggingface.co/Xenova/m2m100_418M | MIT |
| MADLAD-400 3B (ONNX) | Google, Kutalia | https://huggingface.co/Kutalia/madlad400-3b-mt-onnx | Apache-2.0 |
| TranslateGemma 4B (ONNX) | Google, onnx-community | https://huggingface.co/onnx-community/translategemma-text-4b-it-ONNX | Gemma Terms of Use |
| Comic text and bubble detector | ogkalu | https://huggingface.co/ogkalu/comic-text-and-bubble-detector | Apache-2.0 |
| Manga OCR (ONNX) | kha-white (Apache-2.0), l0wgear (conversion, repository states no license) | https://huggingface.co/l0wgear/manga-ocr-2025-onnx | See author column |

Built into the image: ONNX Runtime (Microsoft, MIT). `AI_GPU=vulkan` adds Dawn (Google, BSD-3-Clause); `AI_GPU=nvidia` adds NVIDIA's CUDA libraries under the NVIDIA software license, on the machine that builds the image.

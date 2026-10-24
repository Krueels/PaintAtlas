Based on egui-winit 0.33.3. Local change: emit Event::Paste even when there is no plain-text clipboard content. PaintAtlas handles image-only paste commands; TextEdit receives an empty string and keeps its normal behavior.

Raw Key events are also retained before native Copy/Cut/Paste events so PaintAtlas can remap these keys or use them for held gestures. PaintAtlas deduplicates semantic events outside text editing; TextEdit retains its normal clipboard events.

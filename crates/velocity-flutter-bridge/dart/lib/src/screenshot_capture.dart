import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// Captures the current rendered frame as PNG bytes.
class ScreenshotCapture {
  static Future<Uint8List> capture(WidgetTester tester) async {
    final renderObject = tester.binding.rootElement?.renderObject;
    if (renderObject == null || renderObject is! RenderRepaintBoundary) {
      // Fall back to finding the first RepaintBoundary
      final boundary = find.byType(RepaintBoundary).evaluate().firstOrNull;
      if (boundary == null) {
        return Uint8List(0);
      }
      final ro = boundary.renderObject;
      if (ro is RenderRepaintBoundary) {
        return _captureFromBoundary(ro);
      }
      return Uint8List(0);
    }
    return _captureFromBoundary(renderObject);
  }

  static Future<Uint8List> _captureFromBoundary(RenderRepaintBoundary boundary) async {
    final image = await boundary.toImage(pixelRatio: 1.0);
    final byteData = await image.toByteData(format: ui.ImageByteFormat.png);
    return byteData?.buffer.asUint8List() ?? Uint8List(0);
  }
}

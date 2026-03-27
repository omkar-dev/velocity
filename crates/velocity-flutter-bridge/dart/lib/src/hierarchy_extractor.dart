import 'package:flutter/rendering.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Extracts the current widget tree as a JSON-compatible map
/// matching Velocity's Element schema.
class HierarchyExtractor {
  /// Extract the full element tree from the current render.
  static Map<String, dynamic> extract(WidgetTester tester) {
    final root = tester.binding.rootElement;
    if (root == null) {
      return _emptyElement('Root');
    }
    return _visitElement(root);
  }

  static Map<String, dynamic> _visitElement(Element element) {
    final widget = element.widget;
    final renderObject = element.renderObject;

    // Get bounds from RenderBox
    var x = 0.0, y = 0.0, width = 0.0, height = 0.0;
    if (renderObject is RenderBox && renderObject.hasSize) {
      final size = renderObject.size;
      width = size.width;
      height = size.height;
      try {
        final offset = renderObject.localToGlobal(Offset.zero);
        x = offset.dx;
        y = offset.dy;
      } catch (_) {}
    }

    // Extract text content
    String? text;
    if (widget is Text) {
      text = widget.data ?? widget.textSpan?.toPlainText();
    } else if (widget is RichText) {
      text = widget.text.toPlainText();
    } else if (widget is EditableText) {
      text = widget.controller.text;
    }

    // Extract semantics label
    String? label;
    if (widget is Semantics) {
      label = widget.properties.label;
    }

    // Extract ID from Key
    String? id;
    if (widget.key is ValueKey) {
      id = (widget.key as ValueKey).value?.toString();
    }

    // Check if enabled (for buttons etc.)
    bool enabled = true;
    if (widget is AbsorbPointer) {
      enabled = !widget.absorbing;
    } else if (widget is IgnorePointer) {
      enabled = !widget.ignoring;
    }

    // Recurse children
    final children = <Map<String, dynamic>>[];
    element.visitChildren((child) {
      children.add(_visitElement(child));
    });

    return {
      'id': id,
      'label': label,
      'text': text,
      'type': widget.runtimeType.toString(),
      'bounds': {
        'x': x.round(),
        'y': y.round(),
        'width': width.round(),
        'height': height.round(),
      },
      'enabled': enabled,
      'visible': width > 0 && height > 0,
      'children': children,
    };
  }

  static Map<String, dynamic> _emptyElement(String type) {
    return {
      'id': null,
      'label': null,
      'text': null,
      'type': type,
      'bounds': {'x': 0, 'y': 0, 'width': 0, 'height': 0},
      'enabled': true,
      'visible': false,
      'children': [],
    };
  }
}

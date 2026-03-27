import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'hierarchy_extractor.dart';
import 'screenshot_capture.dart';

/// TCP bridge server that accepts commands from Velocity's Rust bridge.
class VelocityBridgeServer {
  final int port;
  final Widget Function() appBuilder;
  ServerSocket? _server;
  WidgetTester? _tester;

  VelocityBridgeServer({
    required this.port,
    required this.appBuilder,
  });

  /// Start the bridge server. Call this from a Flutter test.
  Future<void> start(WidgetTester tester) async {
    _tester = tester;
    _server = await ServerSocket.bind(InternetAddress.loopbackIPv4, port);
    print('[velocity] Bridge server listening on port $port');

    await for (final socket in _server!) {
      _handleConnection(socket);
    }
  }

  Future<void> _handleConnection(Socket socket) async {
    final lines = socket
        .transform(utf8.decoder)
        .transform(const LineSplitter());

    await for (final line in lines) {
      try {
        final cmd = jsonDecode(line) as Map<String, dynamic>;
        final response = await _handleCommand(cmd);
        socket.writeln(jsonEncode(response));
      } catch (e) {
        socket.writeln(jsonEncode({
          'status': 'error',
          'message': e.toString(),
        }));
      }
    }
  }

  Future<Map<String, dynamic>> _handleCommand(Map<String, dynamic> cmd) async {
    final tester = _tester!;

    switch (cmd['cmd'] as String) {
      case 'init':
        // Pump the app widget
        await tester.pumpWidget(appBuilder());
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'get_hierarchy':
        final tree = HierarchyExtractor.extract(tester);
        return {'status': 'ok', 'data': tree};

      case 'screenshot':
        final pngBytes = await ScreenshotCapture.capture(tester);
        final b64 = base64Encode(pngBytes);
        return {'status': 'ok', 'data': b64};

      case 'tap':
        final x = (cmd['x'] as num).toDouble();
        final y = (cmd['y'] as num).toDouble();
        await tester.tapAt(Offset(x, y));
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'double_tap':
        final x = (cmd['x'] as num).toDouble();
        final y = (cmd['y'] as num).toDouble();
        await tester.tapAt(Offset(x, y));
        await tester.tapAt(Offset(x, y));
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'input_text':
        final text = cmd['text'] as String;
        await tester.enterText(find.byType(EditableText).first, text);
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'clear_text':
        await tester.enterText(find.byType(EditableText).first, '');
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'swipe':
        final fromX = (cmd['from_x'] as num).toDouble();
        final fromY = (cmd['from_y'] as num).toDouble();
        final toX = (cmd['to_x'] as num).toDouble();
        final toY = (cmd['to_y'] as num).toDouble();
        await tester.fling(
          find.byType(Scrollable).first,
          Offset(toX - fromX, toY - fromY),
          1000,
        );
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'pump_frames':
        final count = cmd['count'] as int? ?? 1;
        for (var i = 0; i < count; i++) {
          await tester.pump(const Duration(milliseconds: 16));
        }
        return {'status': 'ok'};

      case 'press_key':
        // Not directly supported in flutter_test, no-op
        return {'status': 'ok'};

      case 'long_press':
        final x = (cmd['x'] as num).toDouble();
        final y = (cmd['y'] as num).toDouble();
        await tester.longPressAt(Offset(x, y));
        await tester.pumpAndSettle();
        return {'status': 'ok'};

      case 'shutdown':
        _server?.close();
        return {'status': 'ok'};

      default:
        return {'status': 'error', 'message': 'Unknown command: ${cmd['cmd']}'};
    }
  }

  void stop() {
    _server?.close();
  }
}

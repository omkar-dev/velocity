import 'package:velocity_flutter_helper/velocity_flutter_helper.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';

void main() {
  testWidgets('HierarchyExtractor extracts widget tree', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Column(
            children: [
              Text('Hello', key: ValueKey('greeting')),
              ElevatedButton(
                onPressed: () {},
                child: Text('Click Me'),
              ),
            ],
          ),
        ),
      ),
    );

    final tree = HierarchyExtractor.extract(tester);
    expect(tree['type'], isNotEmpty);
    expect(tree['visible'], isTrue);
    // Should have children
    expect(tree['children'], isNotEmpty);
  });
}

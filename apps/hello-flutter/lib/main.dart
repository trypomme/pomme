import 'package:flutter/material.dart';

// The same scene as apps/hello-skia, so Flutter (Impeller on Metal) and React
// Native Skia (Ganesh on Metal) can be compared with each other and with the
// iOS Simulator: a dark slate rounded square with a rose → amber gradient
// circle, a white stroked chevron under it, plus a tap counter; and a name
// field along the bottom (the Scaffold lifts it above the keyboard).

void main() => runApp(const HelloApp());

class HelloApp extends StatelessWidget {
  const HelloApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      home: const HelloPage(),
      routes: {'/list': (_) => const ListPage()},
    );
  }
}

class HelloPage extends StatefulWidget {
  const HelloPage({super.key});

  @override
  State<HelloPage> createState() => _HelloPageState();
}

class _HelloPageState extends State<HelloPage> {
  int _taps = 0;
  String _name = '';
  String _submitted = '';

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.white,
      body: Stack(children: [
        Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text('Hello from Flutter', style: TextStyle(fontSize: 17)),
            const SizedBox(height: 16),
            Semantics(
              identifier: 'square',
              button: true,
              child: GestureDetector(
              onTap: () => setState(() => _taps++),
              child: Container(
                width: 300,
                height: 300,
                decoration: BoxDecoration(color: const Color(0xFF1E293B), borderRadius: BorderRadius.circular(32)),
                alignment: Alignment.center,
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    Container(
                      width: 140,
                      height: 140,
                      decoration: const BoxDecoration(
                        shape: BoxShape.circle,
                        gradient: LinearGradient(
                          begin: Alignment.topLeft,
                          end: Alignment.bottomRight,
                          colors: [Color(0xFFF43F5E), Color(0xFFF59E0B)],
                        ),
                      ),
                    ),
                    const CustomPaint(size: Size(300, 300), painter: ChevronPainter()),
                  ],
                ),
              ),
            ),
            ),
            const SizedBox(height: 16),
            Semantics(
              identifier: 'taps',
              child: Text('Tapped $_taps ${_taps == 1 ? 'time' : 'times'}', style: const TextStyle(fontSize: 17)),
            ),
          ],
        ),
      ),
        Positioned(
          left: 16,
          right: 16,
          bottom: 16,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Semantics(
                identifier: 'greeting',
                child: Text(
                _submitted.isNotEmpty ? 'Submitted $_submitted' : (_name.isEmpty ? 'Type your name below' : 'Hello, $_name!'),
                style: const TextStyle(fontSize: 17),
              ),
              ),
              const SizedBox(height: 8),
              Semantics(
                identifier: 'name',
                child: TextField(
                decoration: const InputDecoration(hintText: 'Your name', border: OutlineInputBorder()),
                autocorrect: false,
                onChanged: (v) => setState(() => _name = v),
                onSubmitted: (v) => setState(() => _submitted = v),
              ),
              ),
            ],
          ),
        ),
      ]),
    );
  }
}

/// hello-skia's chevron (120×40, 8 pt round stroke), below the circle here.
class ChevronPainter extends CustomPainter {
  const ChevronPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final path = Path()
      ..moveTo(90, 275)
      ..lineTo(150, 235)
      ..lineTo(210, 275);
    final paint = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.stroke
      ..strokeWidth = 8
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(ChevronPainter oldDelegate) => false;
}

/// Scrolling (the pommeflutter:///list link): 200 rows of 50 pt, each its own
/// colour (red = its index, green = 255 − index) so a screenshot tells which
/// row is where; a tapped row turns black.
class ListPage extends StatefulWidget {
  const ListPage({super.key});

  @override
  State<ListPage> createState() => _ListPageState();
}

class _ListPageState extends State<ListPage> {
  int? _tapped;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: ListView.builder(
        itemCount: 200,
        itemExtent: 50,
        itemBuilder: (context, i) => GestureDetector(
          onTap: () => setState(() => _tapped = i),
          child: Container(color: i == _tapped ? Colors.black : Color.fromARGB(255, i, 255 - i, 128)),
        ),
      ),
    );
  }
}

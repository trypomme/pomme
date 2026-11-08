import 'package:flutter/material.dart';

// The same scene as apps/hello-skia, so Flutter (Impeller on Metal) and React
// Native Skia (Ganesh on Metal) can be compared with each other and with the
// iOS Simulator: a dark slate rounded square with a rose → amber gradient
// circle, a white stroked chevron under it, plus a tap counter.

void main() => runApp(const HelloApp());

class HelloApp extends StatelessWidget {
  const HelloApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(debugShowCheckedModeBanner: false, home: HelloPage());
  }
}

class HelloPage extends StatefulWidget {
  const HelloPage({super.key});

  @override
  State<HelloPage> createState() => _HelloPageState();
}

class _HelloPageState extends State<HelloPage> {
  int _taps = 0;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.white,
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text('Hello from Flutter', style: TextStyle(fontSize: 17)),
            const SizedBox(height: 16),
            GestureDetector(
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
            const SizedBox(height: 16),
            Text('Tapped $_taps ${_taps == 1 ? 'time' : 'times'}', style: const TextStyle(fontSize: 17)),
          ],
        ),
      ),
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

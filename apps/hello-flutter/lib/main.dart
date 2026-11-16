import 'package:flutter/material.dart';

// The same scene as apps/hello-skia, so Flutter (Impeller on Metal) and React
// Native Skia (Ganesh on Metal) can be compared with each other and with the
// iOS Simulator: a dark slate rounded square with a rose → amber gradient
// circle, a white stroked chevron under it, plus a tap counter; and a name
// field along the bottom (the Scaffold lifts it above the keyboard).

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  printJapaneseLines();
  runApp(const HelloApp());
}

class HelloApp extends StatelessWidget {
  const HelloApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      home: const HelloPage(),
      routes: {'/list': (_) => const ListPage(), '/japanese': (_) => const JapanesePage()},
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

/// Japanese text (the pommeflutter:///japanese link): the intltext probe's
/// paragraph in a 200 pt box, as Flutter sets it with the system font's
/// fallback (Hiragino or PingFang on iOS). It also prints where Flutter breaks
/// the paragraph at five widths, each line as `FLUTTER <width>.<line> <text>`,
/// so pomme's tests can check the line rules (printed at launch).
const japanese = '日本語の文章は、単語の間に空白がなくても行の途中で折り返されます。小さい「ょ」や「っ」、句読点は行の頭に来ません。Pommeでも同じように折り返されるはずです。';

/// Where Flutter breaks the paragraph at five widths, and each of its
/// characters' widths as `FLUTTERW U+XXXX <width>` (at launch).
void printJapaneseLines() {
  final seen = <int>{};
  for (final c in '$japanese「」『』（）・ー！？'.runes) {
    if (c < 0x80 || !seen.add(c)) continue;
    final painter = TextPainter(
      text: TextSpan(text: String.fromCharCode(c), style: const TextStyle(fontSize: 17)),
      textDirection: TextDirection.ltr,
    )..layout();
    // ignore: avoid_print
    print('FLUTTERW U+${c.toRadixString(16).toUpperCase().padLeft(4, '0')} ${painter.width.toStringAsFixed(3)}');
  }
  for (final width in [180.0, 200.0, 220.0, 250.0, 300.0]) {
    final painter = TextPainter(
      text: const TextSpan(text: japanese, style: TextStyle(fontSize: 17)),
      textDirection: TextDirection.ltr,
    )..layout(maxWidth: width);
    var line = 0;
    var start = 0;
    for (final m in painter.computeLineMetrics()) {
      final end = painter.getLineBoundary(TextPosition(offset: start)).end;
      // ignore: avoid_print
      print('FLUTTER ${width.toInt()}.$line ${japanese.substring(start, end)} ${m.width.toStringAsFixed(2)}');
      start = end;
      line++;
    }
  }
}

class JapanesePage extends StatelessWidget {
  const JapanesePage({super.key});

  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      backgroundColor: Colors.white,
      body: Padding(
        padding: EdgeInsets.fromLTRB(20, 120, 0, 0),
        child: SizedBox(width: 200, child: Text(japanese, style: TextStyle(fontSize: 17, color: Colors.black))),
      ),
    );
  }
}

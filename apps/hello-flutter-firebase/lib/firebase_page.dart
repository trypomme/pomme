import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:firebase_analytics/firebase_analytics.dart';
import 'package:firebase_auth/firebase_auth.dart';
import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_crashlytics/firebase_crashlytics.dart';
import 'package:flutter/material.dart';

// Firebase through FlutterFire (the page this app opens on): the
// native Firebase Apple SDK, configured from GoogleService-Info.plist. Each
// line runs one module and shows what came back, as hello-expo's screen does.

/// Firebase starts once, on first use.
Future<FirebaseApp>? _starting;
Future<FirebaseApp> start() => _starting ??= Firebase.initializeApp();

/// One anonymous sign-in. Firestore waits for it: its rules allow signed-in
/// users only, and writes made before a sign-in stay with the signed-out user.
Future<UserCredential>? _signingIn;
Future<UserCredential> signIn() async {
  await start();
  return _signingIn ??= FirebaseAuth.instance.signInAnonymously();
}

Future<String> app() async {
  final a = await start();
  // An app ID is 1:<project number>:<platform>:<hash>.
  return '${a.name} · ${a.options.projectId} · ${a.options.appId.split(':')[2]}';
}

Future<String> analytics() async {
  await start();
  final a = FirebaseAnalytics.instance;
  await a.logEvent(name: 'pomme_opened', parameters: {'screen': 'firebase'});
  return 'logged · instance ${await a.appInstanceId ?? 'none'}';
}

Future<String> crashlytics() async {
  await start();
  final c = FirebaseCrashlytics.instance;
  await c.log('opened the Firebase screen');
  await c.setCustomKey('screen', 'firebase');
  return 'logged · collection ${c.isCrashlyticsCollectionEnabled}';
}

Future<String> auth() async {
  final user = (await signIn()).user!;
  return 'anonymous ${user.isAnonymous} · ${user.uid.length} char uid';
}

/// A write and a read of one document: the local cache first, then the server.
Future<String> firestore() async {
  await signIn();
  final ref = FirebaseFirestore.instance.collection('pomme').doc('flutter');
  final at = DateTime.now().millisecondsSinceEpoch;
  final written = ref.set({'greeting': 'hello', 'at': at});
  final cached = (await ref.get(const GetOptions(source: Source.cache))).data();
  await written;
  final server = (await ref.get(const GetOptions(source: Source.server))).data();
  return 'cache ${cached?['greeting']} · server ${server?['at'] == at ? 'same' : 'different'}';
}

class FirebasePage extends StatelessWidget {
  const FirebasePage({super.key});

  @override
  Widget build(BuildContext context) {
    final lines = <(String, Future<String> Function())>[
      ('app', app),
      ('analytics', analytics),
      ('crashlytics', crashlytics),
      ('auth', auth),
      ('firestore', firestore),
    ];
    return Scaffold(
      backgroundColor: Colors.white,
      appBar: AppBar(title: const Text('Firebase')),
      body: ListView(
        children: [for (final (label, run) in lines) Line(label: label, run: run)],
      ),
    );
  }
}

/// "label …" until `run` finishes, then the label and its result (or the error).
class Line extends StatefulWidget {
  const Line({super.key, required this.label, required this.run});
  final String label;
  final Future<String> Function() run;

  @override
  State<Line> createState() => _LineState();
}

class _LineState extends State<Line> {
  late String _text = '${widget.label} …';

  @override
  void initState() {
    super.initState();
    widget.run().then(
      (s) => mounted ? setState(() => _text = '${widget.label} $s') : null,
      onError: (Object e, StackTrace stack) {
        debugPrint('${widget.label} failed: $e\n$stack');
        if (mounted) setState(() => _text = '${widget.label} error $e');
      },
    );
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.all(12),
        child: Text(_text, key: ValueKey(widget.label), style: const TextStyle(fontSize: 15)),
      );
}

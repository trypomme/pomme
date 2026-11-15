import 'package:flutter/material.dart';

import 'firebase_page.dart';

// Firebase from Flutter (FlutterFire), on its own so that hello-flutter needs
// no Firebase SDK and no GoogleService-Info.plist. The app opens on the
// Firebase page.

void main() => runApp(const HelloFirebaseApp());

class HelloFirebaseApp extends StatelessWidget {
  const HelloFirebaseApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(debugShowCheckedModeBanner: false, home: FirebasePage());
  }
}

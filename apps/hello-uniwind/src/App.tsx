// hello-uniwind: Uniwind (Tailwind CSS for React Native) on pomme. The app
// shows one page of the class catalog (src/pages.tsx), chosen by the launch
// link (pommeuniwind:///colors) or the launch argument `-page colors`:
// ///colors, ///borders, ///effects, ///transforms,
// ///text, ///decoration, ///layout, or ///states (variants: active, disabled, focus,
// dark, platform, safe area, data selectors and accent-* color props).
// Without a link it lists the pages.

import './global.css';

import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Linking, Pressable, ScrollView, Settings, Switch, Text, TextInput, View } from 'react-native';
import { SafeAreaListener } from 'react-native-safe-area-context';
import { Uniwind } from 'uniwind';

import { PAGES, type Page, type Tile } from './pages';

function pageFrom(url: string | null): string | null {
  if (!url) return null;
  const m = url.match(/^pommeuniwind:\/\/\/?([a-z]+)/);
  return m ? m[1] : null;
}

export default function App() {
  const [page, setPage] = useState<string | null | undefined>(undefined);

  useEffect(() => {
    // A launch argument (`-page colors`) is in the user defaults, as iOS puts it.
    const fromArgument = Settings.get('page');
    Linking.getInitialURL().then((url) => setPage(typeof fromArgument === 'string' ? fromArgument : pageFrom(url)));
    const sub = Linking.addEventListener('url', ({ url }) => setPage(pageFrom(url)));
    return () => sub.remove();
  }, []);

  if (page === undefined) return null;
  const catalog = PAGES.find((p) => p.name === page);

  return (
    <SafeAreaListener onChange={({ insets }) => Uniwind.updateInsets(insets)}>
      <View className="flex-1 bg-white dark:bg-slate-950 pt-safe">
        <Text testID="title" className="px-3 pt-2 pb-1 text-lg font-bold text-black dark:text-white">
          {page ?? 'hello-uniwind'}
        </Text>
        {catalog ? <Catalog page={catalog} /> : page === 'states' ? <States /> : <Index />}
      </View>
      <StatusBar style="auto" />
    </SafeAreaListener>
  );
}

function Index() {
  return (
    <View className="px-3 gap-2">
      {[...PAGES.map((p) => p.name), 'states'].map((name) => (
        <Text key={name} className="text-base text-blue-600">{`pommeuniwind:///${name}`}</Text>
      ))}
    </View>
  );
}

function Catalog({ page }: { page: Page }) {
  return (
    <ScrollView contentContainerClassName="flex-row flex-wrap px-2 gap-y-3">
      {page.tiles.map((tile, i) => (
        <Cell key={i} index={i} page={page} tile={tile} />
      ))}
    </ScrollView>
  );
}

/** Children shown in a layout tile: three for flex layouts, one otherwise. */
function children(tile: Tile) {
  const many = /flex-(row|col)|justify|space-/.test(tile.cls);
  return Array.from({ length: many ? 3 : 1 }, (_, k) => <View key={k} className={tile.child} />);
}

function Cell({ index, page, tile }: { index: number; page: Page; tile: Tile }) {
  const text = tile.kind === 'text';
  return (
    <View testID={`${page.name}.${index}`} className={text ? 'w-1/2 px-1' : 'w-1/5 items-center'}>
      {text ? (
        <Text className={`h-10 text-base text-black dark:text-white ${tile.cls}`} numberOfLines={tile.cls.includes('clamp') ? undefined : 1}>
          Uniwind text, quickly
        </Text>
      ) : (
        <View className="w-16 h-16">
          {tile.backdrop ? <Backdrop /> : null}
          <View className={`w-16 h-16 ${page.base} ${tile.cls}`}>{tile.child ? children(tile) : null}</View>
        </View>
      )}
      <Text className="mt-1 text-[8px] leading-[10px] text-gray-500 text-center" numberOfLines={2}>
        {tile.cls}
      </Text>
    </View>
  );
}

/** Stripes behind a blend-mode tile: what the tile blends with. */
function Backdrop() {
  return (
    <View className="absolute inset-0 flex-row">
      <View className="flex-1 bg-yellow-300" />
      <View className="flex-1 bg-cyan-500" />
      <View className="flex-1 bg-black" />
      <View className="flex-1 bg-white" />
    </View>
  );
}

function States() {
  const [on, setOn] = useState(true);
  const [selected, setSelected] = useState(false);
  return (
    <View className="px-3 gap-3">
      <Pressable testID="pressable" className="h-12 rounded-lg bg-blue-500 active:bg-red-500 items-center justify-center">
        <Text className="text-white font-semibold">active:bg-red-500</Text>
      </Pressable>
      <Pressable testID="disabled" disabled className="h-12 rounded-lg bg-blue-500 disabled:opacity-40 items-center justify-center">
        <Text className="text-white font-semibold">disabled:opacity-40</Text>
      </Pressable>
      <TextInput
        testID="input"
        placeholder="focus:border-blue-500"
        placeholderTextColorClassName="accent-gray-400"
        cursorColorClassName="accent-red-500"
        className="h-11 px-3 rounded-lg border-2 border-gray-300 focus:border-blue-500 text-black dark:text-white"
      />
      <Pressable
        testID="selected"
        onPress={() => setSelected((s) => !s)}
        data-selected={selected}
        className="h-12 rounded-lg border-2 border-gray-300 data-[selected=true]:bg-emerald-500 data-[selected=true]:border-emerald-700 items-center justify-center"
      >
        <Text className="font-semibold text-black dark:text-white">{`data-[selected=${selected}]`}</Text>
      </Pressable>
      <View className="flex-row gap-3 items-center">
        <View testID="platform" className="w-16 h-16 bg-gray-400 ios:bg-blue-500 android:bg-green-500" />
        <View testID="dark" className="w-16 h-16 bg-amber-300 dark:bg-indigo-700" />
        <View testID="rtl" className="w-16 h-16 bg-gray-200 ltr:border-l-8 rtl:border-r-8 border-red-500" />
        <ActivityIndicator testID="spinner" size="large" colorClassName="accent-red-500" />
        <Switch testID="switch" value={on} onValueChange={setOn} thumbColorClassName="accent-amber-300" trackColorOnClassName="accent-emerald-500" />
      </View>
      <Text className="text-xs text-gray-500">ios:bg-blue-500 · dark:bg-indigo-700 · ltr:border-l-8 · accent-red-500 · accent-amber-300</Text>
    </View>
  );
}

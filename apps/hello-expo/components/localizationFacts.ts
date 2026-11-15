import { getCalendars, getLocales } from 'expo-localization';
import { Settings } from 'react-native';

// The device's language and region as JavaScript sees them: expo-localization's
// first locale and calendar, and Hermes's Intl and Date with no locale given.
// app/localization.tsx shows them. Launched with `-PommeRecordLocalization YES`,
// the app also saves them, one `name value` per line, in its user defaults
// (`PommeLocalization`), so scripts/measure/expo-localization/run.sh can read
// what the Simulator reports.

/** 2026-03-14 09:05:30 UTC, as the locale probe uses. */
const FIXED = new Date(1773479130000);

export function localizationFacts(): [string, string][] {
  const locale = getLocales()[0];
  const calendar = getCalendars()[0];
  const intl = (f: () => string) => {
    try {
      return f();
    } catch (e) {
      return `error ${e}`;
    }
  };
  return [
    ['locales', getLocales().map((l) => l.languageTag).join(',')],
    ['languageTag', locale.languageTag],
    ['languageCode', String(locale.languageCode)],
    ['regionCode', String(locale.regionCode)],
    ['languageRegionCode', String(locale.languageRegionCode)],
    ['currencyCode', String(locale.currencyCode)],
    ['currencySymbol', String(locale.currencySymbol)],
    ['decimalSeparator', String(locale.decimalSeparator)],
    ['digitGroupingSeparator', String(locale.digitGroupingSeparator)],
    ['measurementSystem', String(locale.measurementSystem)],
    ['temperatureUnit', String(locale.temperatureUnit)],
    ['textDirection', String(locale.textDirection)],
    ['calendar', String(calendar.calendar)],
    ['firstWeekday', String(calendar.firstWeekday)],
    ['timeZone', String(calendar.timeZone)],
    ['uses24hourClock', String(calendar.uses24hourClock)],
    ['intl.locale', intl(() => new Intl.DateTimeFormat().resolvedOptions().locale)],
    ['intl.timeZone', intl(() => new Intl.DateTimeFormat().resolvedOptions().timeZone)],
    ['intl.date', intl(() => new Intl.DateTimeFormat().format(FIXED))],
    ['intl.dateTime', intl(() => new Intl.DateTimeFormat(undefined, { dateStyle: 'full', timeStyle: 'long' }).format(FIXED))],
    ['intl.time', intl(() => new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(FIXED))],
    ['intl.number', intl(() => new Intl.NumberFormat().format(1234567.891))],
    ['intl.price', intl(() => new Intl.NumberFormat(undefined, { style: 'currency', currency: locale.currencyCode ?? 'USD' }).format(1234.5))],
    ['date.toLocaleString', intl(() => FIXED.toLocaleString())],
    ['date.offset', String(FIXED.getTimezoneOffset())],
    ['date.hours', String(FIXED.getHours())],
  ];
}

/** With `-PommeRecordLocalization YES`, save the facts for the recording script. */
export function recordLocalizationIfAsked() {
  if (!Settings.get('PommeRecordLocalization')) return;
  const lines = localizationFacts().map(([name, value]) => `${name} ${value}`);
  Settings.set({ PommeLocalization: lines.join('\n') });
}

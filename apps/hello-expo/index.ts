// Polyfills and MSW load before the app (MSW's globals must exist first);
// expo-router/entry comes last, as Expo Router's docs recommend for setup code.
import './mocks';
import 'expo-router/entry';

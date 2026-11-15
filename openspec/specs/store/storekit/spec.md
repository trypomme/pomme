# store/storekit Specification

## Purpose

Answer StoreKit as a phone signed in to the US App Store that has not
bought anything in the app. Analytics and attribution SDKs (Firebase among
them) read the payment queue and report ad attribution at launch, so these
calls must work. Buying is not part of this spec.

Words used here:

- **Payment queue**: `SKPaymentQueue`, where purchases and their
  transactions go.
- **Storefront**: the App Store country the phone uses.
- **SKAdNetwork**: Apple's ad attribution API. Apps register and update a
  "conversion value".

## Requirements

### Requirement: The payment queue has no transactions

`SKPaymentQueue.defaultQueue` SHALL accept and remove transaction observers
and list them. `transactions` SHALL be empty. `canMakePayments` SHALL be
YES. `restoreCompletedTransactions` (with or without a user name) SHALL
finish with no transactions, by calling
`paymentQueueRestoreCompletedTransactionsFinished:` on the observers that
implement it.

#### Scenario: Restore purchases

- **WHEN** an app adds an observer and calls `restoreCompletedTransactions`
- **THEN** the observer hears `paymentQueueRestoreCompletedTransactionsFinished:` and no transactions

### Requirement: The storefront is the US

The queue's `storefront` SHALL have `countryCode` `USA`.

#### Scenario: Country

- **WHEN** an app reads `SKPaymentQueue.defaultQueue.storefront.countryCode`
- **THEN** it is `USA`

### Requirement: Purchases are refused clearly

`addPayment:` SHALL stop the run with a message that names the call,
instead of pretending a purchase happened.

#### Scenario: An app tries to buy

- **WHEN** an app calls `addPayment:`
- **THEN** the run stops and says that `addPayment:` is not supported

### Requirement: Ad attribution succeeds

`SKAdNetwork`'s `registerAppForAdNetworkAttribution`, `updateConversionValue:`
and the `updatePostbackConversionValue:…` methods SHALL succeed, and their
completion handlers SHALL be called with no error.

#### Scenario: Update a postback value

- **WHEN** an SDK calls `updatePostbackConversionValue:completionHandler:` with 3
- **THEN** the handler is called with a nil error

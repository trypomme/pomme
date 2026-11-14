//! StoreKit's payment queue, for a phone signed in to the US App Store that
//! hasn't bought anything in the app: observers can watch the queue, there
//! are no transactions, and restoring purchases finishes with none. Making a
//! purchase (`addPayment:`) isn't modelled yet, and says so.
//!
//! SKAdNetwork (ad attribution, as analytics SDKs report it): registering and
//! updating conversion values succeed, as they do for an app on a phone, and
//! completion handlers hear no error.

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result, Stop};

#[derive(Clone, Default)]
pub struct State {
    queue: u64,
    observers: Vec<u64>,
    storefront: u64,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn default_queue(emu: &mut Emu) -> Result<u64> {
    if emu.storekit.queue == 0 {
        let cls = host_class(emu, "SKPaymentQueue")?;
        emu.storekit.queue = objc::create_instance(emu, cls, 0)?;
    }
    Ok(emu.storekit.queue)
}

/// Tell each observer that responds to `sel` (with the queue as its argument).
fn tell_observers(emu: &mut Emu, sel: &str) -> Result<()> {
    let q = default_queue(emu)?;
    let s = objc::intern(emu, sel)?;
    for o in emu.storekit.observers.clone() {
        if send(emu, o, "respondsToSelector:", &[s])? & 0xff != 0 {
            send(emu, o, sel, &[q])?;
        }
    }
    Ok(())
}

const Q: &str = "SKPaymentQueue";
const AD: &str = "SKAdNetwork";

/// A postback update's completion handler (its last argument), told there was no error.
fn ad_network_done(emu: &mut Emu, handler_arg: usize) -> Result<()> {
    let block = emu.arg(handler_arg);
    if block != 0 {
        crate::libsystem::call_block(emu, block, &[0])?;
    }
    Ok(())
}
const F: &str = "SKStorefront";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (AD, true, "registerAppForAdNetworkAttribution", |_| Ok(())),
    (AD, true, "updateConversionValue:", |_| Ok(())),
    (AD, true, "updatePostbackConversionValue:completionHandler:", |e| ad_network_done(e, 3)),
    (AD, true, "updatePostbackConversionValue:coarseValue:completionHandler:", |e| ad_network_done(e, 4)),
    (AD, true, "updatePostbackConversionValue:coarseValue:lockWindow:completionHandler:", |e| ad_network_done(e, 5)),
    (Q, true, "defaultQueue", |e| {
        let q = default_queue(e)?;
        ret(e, q)
    }),
    (Q, true, "canMakePayments", |e| ret(e, 1)),
    (Q, false, "addTransactionObserver:", |e| {
        let o = e.arg(2);
        if o != 0 && !e.storekit.observers.contains(&o) {
            e.storekit.observers.push(o);
        }
        Ok(())
    }),
    (Q, false, "removeTransactionObserver:", |e| {
        let o = e.arg(2);
        e.storekit.observers.retain(|&x| x != o);
        Ok(())
    }),
    (Q, false, "transactionObservers", |e| {
        let obs = e.storekit.observers.clone();
        let a = foundation::new_array_of(e, &obs)?;
        ret(e, a)
    }),
    (Q, false, "transactions", |e| {
        let a = foundation::new_array_of(e, &[])?;
        ret(e, a)
    }),
    (Q, false, "storefront", |e| {
        if e.storekit.storefront == 0 {
            let cls = host_class(e, F)?;
            e.storekit.storefront = objc::create_instance(e, cls, 0)?;
        }
        let s = e.storekit.storefront;
        ret(e, s)
    }),
    // Nothing to restore: observers hear that restoring finished.
    (Q, false, "restoreCompletedTransactions", |e| tell_observers(e, "paymentQueueRestoreCompletedTransactionsFinished:")),
    (Q, false, "restoreCompletedTransactionsWithApplicationUsername:", |e| tell_observers(e, "paymentQueueRestoreCompletedTransactionsFinished:")),
    (Q, false, "finishTransaction:", |_| Ok(())),
    (Q, false, "addPayment:", |_| Err(Stop::Unsupported("in-app purchases (SKPaymentQueue addPayment:) aren't modelled yet".into()))),
    // The US App Store.
    (F, false, "countryCode", |e| {
        let s = foundation::new_nsstring(e, "USA")?;
        ret(e, s)
    }),
    (F, false, "identifier", |e| {
        let s = foundation::new_nsstring(e, "143441")?;
        ret(e, s)
    }),
];

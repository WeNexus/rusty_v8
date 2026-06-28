use std::ops::{Deref, DerefMut};

use crate::isolate::Isolate;

/// A handle to a shared isolate, allowing access to the isolate in a thread safe way.
///
/// Unlike V8 isolates, these do not currently support re-entrancy.
/// Do not create multiple lockers to the same isolate in the same thread.
#[derive(Debug)]
pub struct Locker<'a> {
  _lock: raw::Locker,
  // We maintain a mut reference to ensure we have exclusive ownership of the isolate during the lock.
  locked: &'a mut Isolate,
}

impl<'a> Locker<'a> {
  /// Claims the isolate, this should only be used from a shared isolate.
  pub fn new(isolate: &'a mut Isolate) -> Self {
    let s = Self {
      _lock: raw::Locker::new(isolate),
      locked: isolate,
    };
    // NOTE(flow v149 port): rusty_v8 v149 removed the slot-based
    // `ScopeData` root mechanism (`ScopeData::new_root`/`drop_root`) that the
    // original v130 patch relied on here. In v149 `HandleScope::new`
    // constructs a scope directly from the isolate (see the `NewHandleScope`
    // trait in `scope.rs`), so no root scope needs to be (re)established when
    // entering the isolate under a lock. We only need to enter the isolate so
    // it becomes `Isolate::GetCurrent()` on this thread.
    unsafe { s.locked.enter() };
    s
  }

  /// Returns a reference to the locked isolate.
  pub fn isolate(&self) -> &Isolate {
    self.locked
  }

  /// Returns a mutable reference to the locked isolate.
  pub fn isolate_mut(&mut self) -> &mut Isolate {
    self.locked
  }

  /// Returns if the isolate is locked by the current thread.
  pub fn is_locked(isolate: &Isolate) -> bool {
    raw::Locker::is_locked(isolate)
  }
}

impl<'a> Drop for Locker<'a> {
  fn drop(&mut self) {
    // A new locker automatically enters the isolate, so be sure to exit the
    // isolate when the locker is dropped. The V8 lock itself is released when
    // the `_lock` field is dropped, which happens after this body runs.
    unsafe { self.exit() };
  }
}

impl<'a> Deref for Locker<'a> {
  type Target = Isolate;
  fn deref(&self) -> &Self::Target {
    self.isolate()
  }
}

impl<'a> DerefMut for Locker<'a> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.isolate_mut()
  }
}

impl<'a> AsMut<Isolate> for Locker<'a> {
  fn as_mut(&mut self) -> &mut Isolate {
    self
  }
}

mod raw {
  use std::mem::MaybeUninit;

  use crate::Isolate;

  // `binding.cc` asserts `sizeof(v8::Locker) == sizeof(size_t) * 2`.
  #[repr(C)]
  #[derive(Debug)]
  pub(super) struct Locker([MaybeUninit<usize>; 2]);

  impl Locker {
    pub fn new(isolate: &Isolate) -> Self {
      unsafe {
        let mut s = Self(MaybeUninit::uninit().assume_init());
        v8__Locker__CONSTRUCT(&mut s, isolate);
        // v8-locker.h disallows copying and assigning, but it does not disallow moving so this is hopefully safe.
        s
      }
    }

    pub fn is_locked(isolate: &Isolate) -> bool {
      unsafe { v8__Locker__IsLocked(isolate) }
    }
  }

  impl Drop for Locker {
    fn drop(&mut self) {
      unsafe { v8__Locker__DESTRUCT(self) }
    }
  }

  unsafe extern "C" {
    fn v8__Locker__CONSTRUCT(locker: *mut Locker, isolate: *const Isolate);
    fn v8__Locker__DESTRUCT(locker: *mut Locker);
    fn v8__Locker__IsLocked(isolate: *const Isolate) -> bool;
  }
}

use crate::{Result, StargridError};

pub type Subscriber<T> = fn(&T) -> Result<()>;

pub struct Publisher<T> {
  subs: Vec<Subscription<T>>,
}

impl<T> Publisher<T> {
  pub fn new() -> Self {
    Self {
      subs: vec![],
    }
  }

  pub fn subscribe(&mut self, subscriber: Subscriber<T>) -> SubscriptionId {
    let subscription = Subscription(self.subs.len(), subscriber);
    let id = SubscriptionId(subscription.0);
    self.subs.push(subscription);
    id
  }

  pub fn unsubscribe(&mut self, subscriber: Subscriber<T>) {
    self.subs.retain(|s| s.1 != subscriber);
  }

  pub fn publish(&mut self, event: T) -> Result<()> {
    let mut errs: Vec<StargridError> = vec![];
    for sub in &self.subs {
      let res = (sub.1)(&event);
      if let Err(err) = res {
        errs.push(err);
      }
    }
    if errs.len() > 0 {
      Err(StargridError::Aggregate(errs))
    } else {
      Ok(())
    }
  }
}

struct Subscription<T>(pub usize, pub Subscriber<T>);

pub struct SubscriptionId(usize);

impl SubscriptionId {
  pub fn unsubscribe_from<T>(&mut self, publisher: &mut Publisher<T>) {
    publisher.subs.retain(|s| s.0 != self.0);
  }
}

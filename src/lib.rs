mod ring_buffer;
use ring_buffer::{RingBufferConsumerRegion, RingBufferProducerRegion};

#[derive(Debug, PartialEq)]
pub struct PopError;

#[derive(Debug, PartialEq)]
pub struct PushError;

pub trait Consumer<T> {
  // Number of slots ready for consuming
  fn slot_count(&self) -> usize;

  // Simple sequential interface to pop slots and sync the internal cursor
  fn pop(&mut self) -> Result<T, PopError>
  where
    T: Copy;

  // Advanced interface to take an snapshot of the slots
  // that are currently available for consumption
  fn region(&mut self) -> RingBufferConsumerRegion<T>;
}

// This represents a frozen view of the Consumer
// It provides with a more advanced interface
// allowing for different styles of consumption
pub trait ConsumerRegion<'a, T> {
  // The number of slots in this region
  fn slot_count(&self) -> usize;

  // Provides next slot and advances the cursor one position within the region
  fn pop(&mut self) -> Result<T, PopError>
  where
    T: Copy;

  // Provides a low level interface to the underlying buffer slices
  fn as_slices(&self) -> (&[T], &[T]);

  // Advances the internal cursor locally in the region
  // but it doesn't need any sync in the Consumer.
  // When the region is dropped then the cursor is synced
  // in the Consumer accordingly.
  fn advance(&mut self, n: usize);
}

pub trait Producer<T> {
  // Number of slots ready for producing
  fn slot_count(&self) -> usize;

  // Simple sequential interface to push slots and sync the internal cursor
  fn push(&mut self, value: T) -> Result<(), PushError>
  where
    T: Copy;

  // Advanced interface to take an snapshot of the slots
  // that are currently available for producing
  fn region(&mut self) -> RingBufferProducerRegion<T>;
}

// This represents a frozen view of the Consumer
// It provides with a more advanced interface
// allowing for different styles of consumption
pub trait ProducerRegion<'a, T> {
  // The number of slots in this region
  fn slot_count(&self) -> usize;

  // Pushes one value to the region
  fn push(&mut self, value: T) -> Result<(), PushError>
  where
    T: Copy;
}

#[cfg(test)]
mod tests {
  use crate::ring_buffer::{RingBuffer, RingBufferConsumerRegion, RingBufferProducerRegion};
  use crate::{Consumer, ConsumerRegion, PopError, Producer, ProducerRegion, PushError};

  struct NonCopyType(i32);
  type CopyType = i32;

  #[test]
  fn ring_buffer_slot_count() {
    let (consumer, producer) = RingBuffer::<NonCopyType>::new(4);
    assert_eq!(0, consumer.slot_count());
    assert_eq!(4, producer.slot_count());
  }

  #[test]
  fn ring_buffer_push_and_pop() {
    let (mut consumer, mut producer) = RingBuffer::<CopyType>::new(2);
    assert_eq!(consumer.pop(), Err(PopError));

    assert_eq!(producer.push(1), Ok(()));
    assert_eq!(producer.push(2), Ok(()));
    assert_eq!(producer.push(3), Err(PushError));
    assert_eq!(producer.slot_count(), 0);

    assert_eq!(consumer.pop(), Ok(1));
    assert_eq!(consumer.slot_count(), 1);
    assert_eq!(consumer.pop(), Ok(2));
    assert_eq!(consumer.slot_count(), 0);
    assert_eq!(consumer.pop(), Err(PopError));

    assert_eq!(producer.slot_count(), 2)
  }

  #[test]
  fn ring_buffer_consumer_region_pop_and_drop() {
    let (mut consumer, mut producer) = RingBuffer::<CopyType>::new(4);

    assert_eq!(producer.push(1), Ok(()));
    assert_eq!(producer.push(2), Ok(()));
    assert_eq!(producer.push(3), Ok(()));

    assert_eq!(producer.slot_count(), 1);

    let mut region = consumer.region();

    assert_eq!(region.pop(), Ok(1));

    assert_eq!(producer.slot_count(), 1);

    assert_eq!(producer.push(4), Ok(()));

    assert_eq!(region.slot_count(), 2);

    drop(region);

    assert_eq!(consumer.slot_count(), 3);
    assert_eq!(producer.slot_count(), 1);

    assert_eq!(consumer.pop(), Ok(2));
  }

  #[test]
  fn ring_buffer_consumer_region_iter() {
    let (mut consumer, mut producer) = RingBuffer::<CopyType>::new(4);

    assert_eq!(producer.push(1), Ok(()));
    assert_eq!(producer.push(2), Ok(()));

    let mut region = consumer.region();

    assert_eq!(region.next(), Some(1));

    let slots: Vec<i32> = region.collect();
    assert_eq!(vec![2], slots);

    assert_eq!(consumer.slot_count(), 0);

    assert_eq!(producer.push(3), Ok(()));
    assert_eq!(producer.push(4), Ok(()));
    assert_eq!(producer.push(5), Ok(()));
    assert_eq!(producer.push(6), Ok(()));

    let region = consumer.region();
    let slots: Vec<i32> = region.collect();
    assert_eq!(slots, vec![3, 4, 5, 6]);

    assert_eq!(consumer.slot_count(), 0);
  }

  #[test]
  fn ring_buffer_consumer_region_advance_and_slices() {
    let (mut consumer, mut producer) = RingBuffer::<CopyType>::new(4);

    assert_eq!(producer.push(1), Ok(()));
    assert_eq!(producer.push(2), Ok(()));
    assert_eq!(producer.push(3), Ok(()));
    assert_eq!(producer.push(4), Ok(()));

    let mut region = consumer.region();

    region.advance(3);

    assert_eq!(region.slot_count(), 1);

    let (s1, s2) = region.as_slices();

    assert_eq!(s1, [4]);
    assert_eq!(s2, []);

    drop(region);

    assert_eq!(producer.push(5), Ok(()));
    assert_eq!(producer.push(6), Ok(()));
    assert_eq!(producer.push(7), Ok(()));

    let mut region = consumer.region();

    let (s1, s2) = region.as_slices();

    assert_eq!(s1, [4]);
    assert_eq!(s2, [5, 6, 7]);

    region.advance(2);

    let (s1, s2) = region.as_slices();

    assert_eq!(s1, [6, 7]);
    assert_eq!(s2, []);

    drop(region);

    assert_eq!(consumer.slot_count(), 2);
  }
}

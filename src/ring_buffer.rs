use std::cell::UnsafeCell;
use std::rc::Rc;

use crate::{Consumer, ConsumerRegion, PopError, Producer, ProducerRegion, PushError};

pub struct RingBuffer<T> {
  buffer: Box<[T]>,
  read_index: usize,
  size: usize,
}

impl<T> RingBuffer<T> {
  pub fn new(capacity: usize) -> (RingBufferConsumer<T>, RingBufferProducer<T>) {
    let mut v = Vec::with_capacity(capacity);
    unsafe { v.set_len(capacity) }

    let buffer = Rc::new(UnsafeCell::new(RingBuffer {
      buffer: v.into_boxed_slice(),
      read_index: 0,
      size: 0,
    }));

    let consumer = RingBufferConsumer {
      buffer: Rc::clone(&buffer),
    };
    let producer = RingBufferProducer { buffer };

    (consumer, producer)
  }

  pub fn read_slots(&self) -> usize {
    self.size
  }

  fn write_slots(&self) -> usize {
    self.buffer.len() - self.size
  }

  fn pop(&mut self) -> Result<T, PopError>
  where
    T: Copy,
  {
    if self.size > 0 {
      let slot = self.buffer[self.read_index];
      self.read_index = (self.read_index + 1) % self.buffer.len();
      self.size -= 1;
      Ok(slot)
    } else {
      Err(PopError)
    }
  }

  fn push(&mut self, slot: T) -> Result<(), PushError> {
    if self.size < self.buffer.len() {
      self.buffer[self.write_index()] = slot;
      self.size += 1;
      Ok(())
    } else {
      Err(PushError)
    }
  }

  fn read_slices(&self) -> (&[T], &[T]) {
    let capacity = self.buffer.len();
    let slice = &self.buffer;
    let write_index = self.write_index();
    if write_index > self.read_index || (write_index == self.read_index && self.size == 0) {
      let range = self.read_index..write_index;
      (&slice[range], &[])
    } else {
      let range1 = self.read_index..capacity;
      let range2 = 0..write_index;
      println!("r1: {:?}, r2: {:?}", range1, range2);
      (&slice[range1], &slice[range2])
    }
  }

  fn write_slices(&mut self) -> (&mut [T], &mut [T]) {
    let write_index = self.write_index();
    let slice = self.buffer.as_mut();
    if self.read_index >= write_index || (write_index == self.read_index && self.size == 0) {
      let range = write_index..self.read_index;
      (&mut slice[range], &mut [])
    } else {
      let (s2, s1) = slice.split_at_mut(write_index);
      (s1, &mut s2[0..self.read_index])
    }
  }

  fn write_index(&self) -> usize {
    (self.read_index + self.size) % self.buffer.len()
  }
}

// Consumer impl ----------------------------

pub struct RingBufferConsumer<T> {
  buffer: Rc<UnsafeCell<RingBuffer<T>>>,
}

impl<T> Consumer<T> for RingBufferConsumer<T> {
  fn slot_count(&self) -> usize {
    self.inner_buffer().read_slots()
  }

  fn pop(&mut self) -> Result<T, PopError>
  where
    T: Copy,
  {
    self.inner_buffer_mut().pop()
  }

  fn region(&mut self) -> RingBufferConsumerRegion<T> {
    let buffer = self.buffer.clone();
    let slices = self.inner_buffer_mut().read_slices();
    RingBufferConsumerRegion {
      buffer,
      slices,
      consumed: 0,
    }
  }
}

impl<T> RingBufferConsumer<T> {
  fn inner_buffer(&self) -> &RingBuffer<T> {
    unsafe { &*self.buffer.get() }
  }

  fn inner_buffer_mut(&mut self) -> &mut RingBuffer<T> {
    unsafe { &mut *self.buffer.get() }
  }
}

pub struct RingBufferConsumerRegion<'a, T> {
  buffer: Rc<UnsafeCell<RingBuffer<T>>>,
  slices: (&'a [T], &'a [T]),
  consumed: usize,
}

impl<T> Drop for RingBufferConsumerRegion<'_, T> {
  fn drop(&mut self) {
    let buffer = unsafe { &mut *self.buffer.get() };
    buffer.read_index += self.consumed;
    buffer.size -= self.consumed;
  }
}

impl<T> ConsumerRegion<'_, T> for RingBufferConsumerRegion<'_, T> {
  fn slot_count(&self) -> usize {
    self.slices.0.len() + self.slices.1.len() - self.consumed
  }

  // Provides next slot and advances the cursor one position within the region
  fn pop(&mut self) -> Result<T, PopError>
  where
    T: Copy,
  {
    let capacity = self.slices.0.len() + self.slices.1.len();
    let index = self.consumed;
    if index >= capacity {
      Err(PopError)
    } else {
      self.consumed += 1;
      if index < self.slices.0.len() {
        Ok(self.slices.0[index])
      } else {
        Ok(self.slices.1[index - self.slices.0.len()])
      }
    }
  }

  fn as_slices(&self) -> (&[T], &[T]) {
    let (s1, s2) = self.slices;
    if self.consumed < s1.len() {
      (&s1[self.consumed..s1.len()], s2)
    } else {
      (&s2[self.consumed - s1.len()..s2.len()], &[])
    }
  }

  fn advance(&mut self, n: usize) {
    self.consumed += self.slot_count().min(n);
  }
}

// This provides sequential access with automatic advance of the cursor
// within the region, without requiring sync within the Consumer.
// An alternative or complement to the Iterator would be to have a pop method
// within the region.
impl<T> Iterator for RingBufferConsumerRegion<'_, T>
where
  T: Copy,
{
  type Item = T;

  // Provides next slot and advances the cursor one position within the region
  fn next(&mut self) -> Option<Self::Item> {
    match self.pop() {
      Err(PopError) => None,
      Ok(value) => Some(value),
    }
  }
}

// Producer impl --------------------------------

pub struct RingBufferProducer<T> {
  buffer: Rc<UnsafeCell<RingBuffer<T>>>,
}

impl<T> Producer<T> for RingBufferProducer<T> {
  fn slot_count(&self) -> usize {
    self.inner_buffer().write_slots()
  }

  fn push(&mut self, value: T) -> Result<(), PushError>
  where
    T: Copy,
  {
    self.inner_buffer_mut().push(value)
  }

  fn region(&mut self) -> RingBufferProducerRegion<T> {
    let buffer = self.buffer.clone();
    let slices = self.inner_buffer_mut().write_slices();
    RingBufferProducerRegion {
      buffer,
      slices,
      produced: 0,
    }
  }
}

impl<T> RingBufferProducer<T> {
  fn inner_buffer(&self) -> &RingBuffer<T> {
    unsafe { &*self.buffer.get() }
  }

  fn inner_buffer_mut(&mut self) -> &mut RingBuffer<T> {
    unsafe { &mut *self.buffer.get() }
  }
}

pub struct RingBufferProducerRegion<'a, T> {
  buffer: Rc<UnsafeCell<RingBuffer<T>>>,
  slices: (&'a mut [T], &'a mut [T]),
  produced: usize,
}

impl<T> Drop for RingBufferProducerRegion<'_, T> {
  fn drop(&mut self) {
    let buffer = unsafe { &mut *self.buffer.get() };
    buffer.size += self.produced;
  }
}

impl<'a, T> ProducerRegion<'a, T> for RingBufferProducerRegion<'a, T> {
  fn slot_count(&self) -> usize {
    self.slices.0.len() + self.slices.1.len() - self.produced
  }

  fn push(&mut self, value: T) -> Result<(), PushError>
  where
    T: Copy,
  {
    let capacity = self.slices.0.len() + self.slices.1.len();
    let index = self.produced;
    if index >= capacity {
      Err(PushError)
    } else {
      self.produced += 1;
      if index < self.slices.0.len() {
        self.slices.0[index] = value;
      } else {
        self.slices.1[index - self.slices.0.len()] = value;
      }
      Ok(())
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{
    RingBuffer, RingBufferConsumer, RingBufferConsumerRegion, RingBufferProducer,
    RingBufferProducerRegion,
  };
  use crate::{Consumer, ConsumerRegion, PopError, Producer, ProducerRegion, PushError};
  use std::cell::UnsafeCell;
  use std::rc::Rc;

  struct NonCopyType(i32);
  type CopyType = i32;

  #[test]
  fn consumer_region_slot_count_for_single_buffer() {
    let region = new_consumer_region1();
    assert_eq!(2, region.slot_count())
  }

  #[test]
  fn consumer_region_slot_count_for_split_buffer() {
    let region = new_consumer_region2();
    assert_eq!(3, region.slot_count())
  }

  #[test]
  fn consumer_region_iterator_for_single_buffer() {
    let region = new_consumer_region1();
    let slots: Vec<i32> = region.collect();
    assert_eq!(vec![1, 2], slots);
  }

  #[test]
  fn consumer_region_iterator_for_split_buffer() {
    let region = new_consumer_region2();
    let slots: Vec<i32> = region.collect();
    assert_eq!(vec![3, 0, 1], slots);
  }

  #[test]
  fn consumer_region_pop_and_iterator() {
    let mut region = new_consumer_region2();
    assert_eq!(region.pop(), Ok(3));
    let slots: Vec<i32> = region.collect();
    assert_eq!(vec![0, 1], slots);
  }

  #[test]
  fn consumer_region_pop_and_advance() {
    let mut region = new_consumer_region2();
    assert_eq!(region.pop(), Ok(3));
    region.advance(2);
    assert_eq!(region.pop(), Err(PopError));
    let slots: Vec<i32> = region.collect();
    assert_eq!(Vec::<i32>::with_capacity(0), slots);
  }

  #[test]
  fn consumer_region_as_slices1() {
    let region = new_consumer_region1();
    let (s1, s2) = region.as_slices();
    assert_eq!(&V[1..3], s1);
    assert_eq!(&E, s2);
  }

  #[test]
  fn consumer_region_as_slices2() {
    let region = new_consumer_region2();
    assert_eq!((&V[3..4], &V[0..2]), region.as_slices());
  }

  #[test]
  fn consumer_region_advance() {
    let mut region = new_consumer_region2();
    let (s1, s2) = region.as_slices();
    assert_eq!(&V[3..4], s1);
    assert_eq!(&V[0..2], s2);

    region.advance(2);
    let (s1, s2) = region.as_slices();
    assert_eq!(&V[1..2], s1);
    assert_eq!(&E, s2);
  }

  #[test]
  fn producer_region_slot_count_for_single_buffer() {
    let mut s1 = [1, 2];
    let mut s2 = [];
    let region = new_producer_region(&mut s1, &mut s2);
    assert_eq!(2, region.slot_count())
  }

  #[test]
  fn producer_region_slot_count_for_split_buffer() {
    let mut s1 = [3];
    let mut s2 = [0, 1];
    let region = new_producer_region(&mut s1, &mut s2);
    assert_eq!(3, region.slot_count());
  }

  #[test]
  fn producer_region_push() {
    let mut s1 = [3];
    let mut s2 = [0, 1];
    {
      let mut region = new_producer_region(&mut s1, &mut s2);
      assert_eq!(region.push(10), Ok(()));
      assert_eq!(region.push(11), Ok(()));
      assert_eq!(region.push(12), Ok(()));
      assert_eq!(region.push(13), Err(PushError));
      assert_eq!(region.slot_count(), 0);
    }
    assert_eq!(s1, [10]);
    assert_eq!(s2, [11, 12]);
  }

  const E: [i32; 0] = [];
  const V: [i32; 5] = [0, 1, 2, 3, 4];

  fn new_ring_buffer(size: usize) -> Rc<UnsafeCell<RingBuffer<i32>>> {
    Rc::new(UnsafeCell::new(RingBuffer {
      buffer: Box::new([]),
      read_index: 0,
      size,
    }))
  }

  fn new_consumer_region1<'a>() -> RingBufferConsumerRegion<'a, i32> {
    RingBufferConsumerRegion {
      buffer: new_ring_buffer(2),
      slices: (&V[1..3], &E),
      consumed: 0,
    }
  }

  fn new_consumer_region2<'a>() -> RingBufferConsumerRegion<'a, i32> {
    RingBufferConsumerRegion {
      buffer: new_ring_buffer(3),
      slices: (&V[3..4], &V[0..2]),
      consumed: 0,
    }
  }

  fn new_producer_region<'a>(
    s1: &'a mut [i32],
    s2: &'a mut [i32],
  ) -> RingBufferProducerRegion<'a, i32> {
    RingBufferProducerRegion {
      buffer: new_ring_buffer(s1.len() + s2.len()),
      slices: (s1, s2),
      produced: 0,
    }
  }
}

use bit_vec::BitVec;

pub struct Bits2d {
    width: usize,
    height: usize,
    bv: BitVec
}

impl Bits2d {
    pub fn new(width: usize, height: usize) -> Bits2d {
        Bits2d {
            width,
            height,
            bv: BitVec::from_elem(width*height, false)
        }
    }
    pub fn get(&self, x: usize, y: usize) -> Option<bool> {
        self.bv.get(y*self.width+x)
    }
    pub fn set(&mut self, x: usize, y: usize, b: bool) {
        self.bv.set(y*self.width+x, b);
    }
    pub fn set_all_zero(&mut self) {
        self.bv.clear();
    }
    pub fn set_all_one(&mut self) {
        self.bv.set_all();
    }
    /// Bits will be pretty meaningless after this,
    /// so make sure you rewrite them all before reading them
    pub fn resize(&mut self, width: usize, height: usize, new_bit_if_growing: bool) {
        use std::cmp::Ordering::*;
        self.width = width;
        self.height = height;
        let product = width*height;
        match product.cmp(&self.bv.len()) {
            Less => self.bv.truncate(product),
            Equal => {},
            Greater => self.bv.grow(product-self.bv.len(), new_bit_if_growing),
        }
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn area(&self) -> usize {
        self.bv.len()
    }
}

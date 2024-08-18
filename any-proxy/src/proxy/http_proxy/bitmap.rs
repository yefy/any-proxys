use anyhow::anyhow;
use anyhow::Result;

#[derive(Clone)]
pub struct BitMap {
    pub bitset: Vec<u8>,
    pub slice_size: usize,
    pub curr_slice_size: usize,
}

impl BitMap {
    pub fn new(byte_size: usize, slice_size: usize) -> Self {
        BitMap {
            bitset: vec![0u8; byte_size],
            slice_size,
            curr_slice_size: 0,
        }
    }

    pub fn from_slice(size: u64, slice: u64) -> Result<Self> {
        let (byte_size, slice_size) = bitmap_size(size, slice)?;
        Ok(Self::new(byte_size, slice_size))
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bitset.as_slice()
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.bitset.as_mut_slice()
    }

    pub fn repair(&mut self) {
        self.curr_slice_size = 0;
        for index in 0..self.slice_size {
            if self.get_bit(index) {
                self.curr_slice_size += 1;
            }
        }
    }

    pub fn is_full(&self) -> bool {
        self.slice_size == self.curr_slice_size
    }
    pub fn size(&self) -> usize {
        self.bitset.len()
    }
    pub fn slice_size(&self) -> usize {
        self.slice_size
    }
    pub fn to_string(&self) -> String {
        let mut string = String::new();
        for index in 0..self.slice_size {
            let str = if self.get_bit(index) {
                1.to_string()
            } else {
                0.to_string()
            };
            string.push_str(&str);
        }
        return string;
    }

    // 设置指定索引位置的比特位值
    pub fn set_bit(&mut self, index: usize, value: bool) {
        if self.get_bit(index) == value {
            return;
        }
        self.curr_slice_size += 1;
        if self.curr_slice_size > self.slice_size {
            log::error!(
                "self.curr_slice_size:{}, self.slice_size:{}",
                self.curr_slice_size,
                self.slice_size
            );
        }

        let byte_index = index / 8;
        let bit_index = index % 8;

        if value {
            self.bitset[byte_index] |= 1 << bit_index;
        } else {
            self.bitset[byte_index] &= !(1 << bit_index);
        }
    }

    // 获取指定索引位置的比特位值
    pub fn get_bit(&self, index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = index % 8;
        (self.bitset[byte_index] & (1 << bit_index)) != 0
    }
}
// slize 100,   1=1   99=1  100=1,   101 = 2,   199=2  200=2  201=3
// u8=8     1=1  8=1    9=2
pub fn bitmap_size(size: u64, slice: u64) -> Result<(usize, usize)> {
    if size <= 0 {
        return Ok((0, 0));
    }
    let slice_size = (size - 1) / slice + 1;
    let byte_size = (slice_size - 1) / 8 + 1;
    Ok((byte_size as usize, slice_size as usize))
}

pub fn align_bitset_start_index(range_start: u64, slice: u64) -> Result<usize> {
    if range_start % slice != 0 {
        return Err(anyhow!("not align"));
    }
    //100,  199,      index = 1  size = 1
    let index = (range_start / slice) as usize;
    return Ok(index);
}
//slice 100,    0, 199     100, 199
pub fn align_bitset_index(range_start: u64, range_end: u64, slice: u64) -> Result<Vec<usize>> {
    if range_start >= range_end {
        return Err(anyhow!("range_start >= range_end"));
    }
    if range_start % slice != 0 || (range_end + 1) % slice != 0 {
        return Err(anyhow!("not align"));
    }
    //100,  199,      index = 1  size = 1
    let index = (range_start / slice) as usize;
    let size = ((range_end - range_start + 1) / slice) as usize;
    let mut bitset = Vec::with_capacity(size);
    for v in 0..size {
        bitset.push(index + v);
    }
    return Ok(bitset);
}

pub fn is_first_n_bits_set(byte: u8, n: u8) -> bool {
    if n > 8 {
        panic!("n > 8")
    }

    // 创建一个掩码，其前 n 位为 1，其他位为 0
    let mask: u16 = 1 << n;
    let mask = (mask - 1) as u8;

    // 将 byte 与掩码进行与运算，并判断是否等于掩码
    (byte & mask) == mask
}

pub fn is_first_n_bits_set_skip(byte: u8, skip: u8, n: u8) -> bool {
    if skip + n > 8 {
        panic!("skip + n != 8")
    }
    let byte = byte >> skip;
    is_first_n_bits_set(byte, n)
}

pub fn align_bitset_end_index(
    bitmap: &BitMap,
    range_start: u64,
    range_end: u64,
    slice_end: u64,
    slice: u64,
) -> Result<u64> {
    if range_start >= range_end {
        return Err(anyhow!("range_start >= range_end"));
    }
    if range_start % slice != 0 || (range_end + 1) % slice != 0 || (slice_end + 1) % slice != 0 {
        return Err(anyhow!("not align"));
    }
    //0   99,
    //100,  199,      index = 1  size = 1
    let mut index = (range_start / slice) as usize;
    let mut size = ((slice_end - range_start + 1) / slice) as usize;
    let mut len = 0;
    loop {
        if size <= 0 {
            break;
        }
        let byte_index = index / 8;
        if byte_index >= bitmap.bitset.len() {
            break;
        }
        let bit_index = (index % 8) as u8;
        let skip = bit_index;
        let num = 8 - skip;
        let num = std::cmp::min(size, num as usize) as u8;
        let b = bitmap.bitset[byte_index];
        if is_first_n_bits_set_skip(b, skip, num) {
            size -= num as usize;
            len += num as usize;
            index += num as usize;
        } else {
            let mut num = num;
            loop {
                num = num - 1;
                if num <= 0 {
                    break;
                }
                if is_first_n_bits_set_skip(b, skip, num) {
                    len += num as usize;
                    break;
                }
            }
            break;
        }
    }

    let size = ((range_end - range_start + 1) / slice) as usize;
    let len = len / size * size;
    if len <= 0 {
        return Ok(0);
    }
    let range_end = range_start + len as u64 * slice - 1;

    return Ok(range_end);
}

pub fn align_bitset_ok(
    bitmap: &BitMap,
    range_start: u64,
    range_end: u64,
    slice: u64,
) -> Result<bool> {
    let indexs = align_bitset_index(range_start, range_end, slice)?;
    for index in &indexs {
        if !bitmap.get_bit(*index) {
            //log::debug!(target: "main", "align_bitset_ok false {}:{:?}, range_start:{}, range_end:{}, slice_size:{}, bitmap:{}", index, indexs, range_start, range_end, bitmap.slice_size,bitmap.to_string());
            return Ok(false);
        }
    }
    return Ok(true);
}

pub fn update_bitset(
    bitmap: &mut BitMap,
    range_start: u64,
    range_end: u64,
    slice: u64,
    skip_bitset_index: i64,
    file_length: u64,
) -> Result<bool> {
    if range_start > range_end {
        return Err(anyhow!("range_start >= range_end"));
    }
    if range_start == range_end {
        return Ok(false);
    }

    //  5-6 => nil,    5-100 => 0     5-101 => 0,    5-199 => 0,1

    //5 99,  index = 0, size = 1 => 0,
    //5 100,  index = 0, size = 1 => 0,
    //5 199,  index = 0, size = 2 => 0, 1
    //5 201,  index = 0, size = 2 => 0, 1
    let index = (range_start / slice) as usize;
    let size = ((range_end + 1) / slice) as usize;
    let mut indexs = Vec::with_capacity(size - index);
    for v in index..size {
        let bit_index = v;
        if skip_bitset_index > 0 && skip_bitset_index == bit_index as i64 {
            continue;
        }
        indexs.push(bit_index);
    }

    if range_end + 1 == file_length {
        let bit_index = range_end / slice;
        indexs.push(bit_index as usize);
    }

    for index in &indexs {
        bitmap.set_bit(*index, true);
    }

    if indexs.len() > 0 {
        //log::debug!(target: "main", "update_bitset true skip_bitset_index:{}, file_length:{}, {:?}, range_start:{}, range_end:{}, slice_size:{}, bitmap:{}", skip_bitset_index, file_length,indexs, range_start, range_end, bitmap.slice_size,bitmap.to_string());
        return Ok(true);
    }

    return Ok(false);
}

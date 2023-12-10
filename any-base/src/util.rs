pub struct Vecs {
    datas: Vec<Vec<u8>>,
    len: usize,
}

impl Vecs {
    pub fn new() -> Self {
        Vecs {
            datas: Vec::with_capacity(10),
            len: 0,
        }
    }
    pub fn push(&mut self, data: Vec<u8>) {
        self.len += data.len();
        self.datas.push(data);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn to_vec(&mut self) -> Vec<u8> {
        if self.datas.len() == 1 {
            return self.datas.pop().unwrap();
        }

        let mut data = Vec::with_capacity(self.len);
        for v in &self.datas {
            data.extend_from_slice(v.as_slice());
        }
        data
    }
}

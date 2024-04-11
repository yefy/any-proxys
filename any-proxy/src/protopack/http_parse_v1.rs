use any_base::io_rb::buf_reader::BufReader;
use any_base::util::buf_split_once;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;

pub async fn read_host<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
    check_methods: &HashMap<String, bool>,
) -> Result<Option<String>> {
    buf_reader.start();
    let domain = do_read_host(buf_reader, check_methods).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if domain.is_err() {
        Ok(None)
    } else {
        Ok(Some(domain?))
    }
}

async fn do_read_host<R: AsyncRead + Unpin>(
    buf_reader: &mut R,
    check_methods: &HashMap<String, bool>,
) -> Result<String> {
    let mut bytes = Bytes::new(buf_reader);
    if !check_methods.is_empty() {
        skip_space(&mut bytes).await?;
        let size = find_space(&mut bytes, 20).await?;
        let method = bytes.peek_n(size).await?;
        let method = String::from_utf8(method.to_vec())?.trim().to_lowercase();
        let value = check_methods.get(&method);
        if value.is_none() {
            return Err(anyhow::anyhow!("None"));
        }

        if !*value.unwrap() {
            return Err(anyhow::anyhow!("None"));
        }
        bytes.advance(size);
    }

    let size = find_lines(&mut bytes, 1024 * 16).await?;
    bytes.advance(size + 2);

    loop {
        let size = find_lines(&mut bytes, 1024 * 16).await?;
        let lines = bytes.peek_n(size).await?;
        let lines = buf_split_once(lines, b":");
        if lines.is_none() {
            return Err(anyhow::anyhow!("None"));
        }
        let (key, value) = lines.unwrap();
        let key = String::from_utf8(key.to_vec())?.trim().to_lowercase();
        if key == "host" {
            let value = String::from_utf8(value.to_vec())?;
            let value = value.trim();
            return Ok(value.to_string());
        }
        bytes.advance(size + 2);
    }
}

async fn skip_space<R: AsyncRead + Unpin>(bytes: &mut Bytes<'_, R>) -> Result<()> {
    loop {
        let d = bytes.peek().await?;
        if d == b' ' {
            bytes.advance(1);
            continue;
        }
        break;
    }
    Ok(())
}

async fn find_space<R: AsyncRead + Unpin>(
    bytes: &mut Bytes<'_, R>,
    max_size: usize,
) -> Result<usize> {
    let pos = bytes.pos();
    loop {
        let d = bytes.peek().await?;
        if d == b' ' {
            break;
        }
        bytes.advance(1);
        if bytes.pos() - pos > max_size {
            return Err(anyhow::anyhow!("bytes max len"));
        }
    }
    let size = bytes.pos() - pos;
    bytes.set_pos(pos);
    Ok(size)
}

async fn find_lines<R: AsyncRead + Unpin>(
    bytes: &mut Bytes<'_, R>,
    max_size: usize,
) -> Result<usize> {
    let pos = bytes.pos();
    loop {
        let d = bytes.peek_n(2).await?;
        if d == b"\r\n" {
            break;
        }
        bytes.advance(1);
        if bytes.pos() - pos > max_size {
            return Err(anyhow::anyhow!("bytes max len"));
        }
    }
    let size = bytes.pos() - pos;
    bytes.set_pos(pos);
    Ok(size)
}

pub struct Bytes<'a, R: AsyncRead + Unpin> {
    buf_reader: &'a mut R,
    slice: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<'a, R: AsyncRead + Unpin> Bytes<'a, R> {
    #[inline]
    pub fn new(buf_reader: &'a mut R) -> Bytes<'a, R> {
        Bytes {
            buf_reader,
            slice: vec![0; 1024 * 16],
            pos: 0,
            cap: 0,
        }
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos
    }

    #[inline]
    pub fn pos_data(&self, pos: usize) -> &[u8] {
        debug_assert!(pos < self.pos, "overflow");
        &self.slice[pos..self.pos]
    }

    #[inline]
    pub async fn peek(&mut self) -> Result<u8> {
        self.peek_ahead(0).await
    }

    #[inline]
    pub async fn peek_ahead(&mut self, n: usize) -> Result<u8> {
        self.fill_buf(n + 1).await?;
        debug_assert!(self.pos + n <= self.cap, "overflow");
        Ok(self.slice[self.pos + n])
    }

    #[inline]
    pub async fn peek_n(&mut self, n: usize) -> Result<&[u8]> {
        self.fill_buf(n).await?;
        debug_assert!(self.pos + n <= self.cap, "overflow");
        Ok(&self.slice[self.pos..self.pos + n])
    }

    #[inline]
    pub async fn next(&mut self) -> Result<u8> {
        let data = self.peek().await?;
        self.bump();
        Ok(data)
    }

    #[inline]
    pub fn bump(&mut self) {
        debug_assert!(self.pos < self.cap, "overflow");
        self.pos += 1;
    }

    #[allow(unused)]
    #[inline]
    pub fn advance(&mut self, n: usize) {
        debug_assert!(self.pos + n <= self.cap, "overflow");
        self.pos += n;
    }

    #[inline]
    pub async fn fill_buf(&mut self, n: usize) -> Result<()> {
        if self.cap >= self.slice.len() {
            return Err(anyhow::anyhow!("fill_buf"));
        }

        if self.len() >= n {
            return Ok(());
        }

        if self.slice.len() - self.pos < n {
            return Err(anyhow::anyhow!("fill_buf"));
        }
        let n = n - self.len();
        self.buf_reader
            .read_exact(&mut self.slice[self.cap..self.cap + n])
            .await?;
        self.cap += n;
        return Ok(());
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cap - self.pos
    }
}

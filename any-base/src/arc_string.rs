use core::ops::RangeBounds;
use mysql_common::value::convert::FromValue;
use mysql_common::value::Value;
use serde::de::{Deserialize, Deserializer};
use serde::ser::Serializer;
use serde::Serialize;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ops::Range;
use std::sync::Arc;

struct ArcStringContext {
    data: Arc<String>,
    range: Range<usize>,
}

#[derive(Clone)]
pub struct ArcString {
    ctx: Arc<ArcStringContext>,
}

impl ArcString {
    pub fn new(str: String) -> Self {
        let len = str.len();
        Self {
            ctx: Arc::new(ArcStringContext {
                data: Arc::new(str),
                range: Range { start: 0, end: len },
            }),
        }
    }

    pub fn len(&self) -> usize {
        self.ctx.range.end - self.ctx.range.start
    }

    pub fn as_str(&self) -> &str {
        &self.ctx.data[self.ctx.range.start..self.ctx.range.end]
    }

    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    pub fn slice(&self, range: impl RangeBounds<usize>) -> Self {
        use core::ops::Bound;

        let len = self.len();

        let begin = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => len,
        };

        assert!(
            begin <= end,
            "range start must not be greater than end: {:?} <= {:?}",
            begin,
            end,
        );
        assert!(
            end <= len,
            "range end out of bounds: {:?} <= {:?}",
            end,
            len,
        );

        if begin == end {
            return Self::default();
        }

        let diff_start = begin - 0;
        let diff_end = len - end;

        Self {
            ctx: Arc::new(ArcStringContext {
                data: self.ctx.data.clone(),
                range: Range {
                    start: self.ctx.range.start + diff_start,
                    end: self.ctx.range.end - diff_end,
                },
            }),
        }
    }
}

impl Default for ArcString {
    #[inline]
    fn default() -> Self {
        Self::new("".to_string())
    }
}

impl Deref for ArcString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<[u8]> for ArcString {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes() // 将 `&str` 转换为字节切片
    }
}

impl From<String> for ArcString {
    #[inline]
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for ArcString {
    #[inline]
    fn from(s: &str) -> Self {
        let s = s.to_owned();
        Self::new(s)
    }
}

impl Hash for ArcString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state); // 直接对 `as_str` 进行哈希
    }
}

impl Eq for ArcString {}

impl PartialEq for ArcString {
    fn eq(&self, other: &ArcString) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
    fn ne(&self, other: &ArcString) -> bool {
        PartialEq::ne(&self[..], &other[..])
    }
}

impl fmt::Debug for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Drain").field(&self.as_str()).finish()
    }
}

impl fmt::Display for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl FromValue for ArcString {
    type Intermediate = String; // 使用 String 作为 Intermediate 类型

    // fn from_value_opt(value: Value) -> Result<Self, FromValueError> {
    //     match value {
    //         Value::Bytes(bytes) => {
    //             // 将字节数组转换为 String，然后转换为 ArcString
    //             let string = String::from_utf8(bytes)
    //                 .map_err(|e| FromValueError(Value::Bytes(e.into_bytes())))?;
    //             Ok(ArcString::from(string))
    //         }
    //         _ => Err( FromValueError(Value::Bytes("Expected String or Bytes".as_bytes().to_vec()))),
    //     }
    // }
    //
    // fn get_intermediate(value: Value) -> Result<Self::Intermediate, FromValueError> {
    //     // 获取中间类型的转换
    //     Self::Intermediate::try_from(value)
    // }
}

impl From<ArcString> for Value {
    fn from(arc_str: ArcString) -> Value {
        Value::Bytes(arc_str.as_str().as_bytes().to_vec()) // 将 ArcString 转换为字节表示
    }
}

impl Serialize for ArcString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArcString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ArcString::new(s))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::Arc;

    //cargo test test_arc_string -- --nocapture
    //cargo test test_arc_string --release -- --nocapture
    #[tokio::test]
    async fn test_arc_string() {
        let str = "abcdefgh";
        let arc_string = ArcString::from(str);
        println!("arc_string:{}, str:{}", arc_string, str);
        assert_eq!(arc_string.len(), str.len());
        assert_eq!(arc_string.as_str(), str);
        assert_eq!(arc_string.to_string(), str.to_string());
        assert_eq!(&arc_string[..], str);

        let str1 = str.clone();
        let arc_string1 = arc_string.clone();
        println!("arc_string1:{}, str1:{}", arc_string1, str1);
        assert_eq!(arc_string1.len(), str1.len());
        assert_eq!(arc_string1.as_str(), str1);
        assert_eq!(arc_string1.to_string(), str1.to_string());
        assert_eq!(&arc_string1[..], str1);

        let str0 = &str[..];
        let arc_string0 = arc_string.slice(..);
        println!("arc_string0:{}, str0:{}", arc_string0, str0);
        assert_eq!(arc_string0.len(), str0.len());
        assert_eq!(arc_string0.as_str(), str0);
        assert_eq!(arc_string0.to_string(), str0.to_string());
        assert_eq!(&arc_string0[..], str0);

        let str2 = &str[3..7];
        let arc_string2 = arc_string.slice(3..7);
        println!("arc_string2:{}, str2:{}", arc_string2, str2);
        assert_eq!(arc_string2.len(), str2.len());
        assert_eq!(arc_string2.as_str(), str2);
        assert_eq!(arc_string2.to_string(), str2.to_string());
        assert_eq!(&arc_string2[..], str2);

        let str3 = &str2[1..3];
        let arc_string3 = arc_string2.slice(1..3);
        println!("arc_string3:{}, str3:{}", arc_string3, str3);
        assert_eq!(arc_string3.len(), str3.len());
        assert_eq!(arc_string3.as_str(), str3);
        assert_eq!(arc_string3.to_string(), str3.to_string());
        assert_eq!(&arc_string3[..], str3);
        {
            // 测试空字符串
            //#[tokio::test]
            //async fn test_empty_string() {
            println!("test_empty_string");
            {
                let empty_str = "";
                let arc_string = ArcString::from(empty_str);
                assert_eq!(arc_string.len(), 0);
                assert_eq!(arc_string.as_str(), empty_str);
                assert_eq!(arc_string.to_string(), empty_str.to_string());
                assert_eq!(&arc_string[..], empty_str);
            }

            // 测试整个字符串切片
            //#[tokio::test]
            //async fn test_full_string_slice() {
            println!("test_full_string_slice");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);
                let slice = arc_string.slice(..);
                assert_eq!(slice.len(), str.len());
                assert_eq!(slice.as_str(), str);
                assert_eq!(slice.to_string(), str.to_string());
                assert_eq!(&slice[..], str);
            }

            // 测试部分字符串切片
            //#[tokio::test]
            //async fn test_partial_string_slice() {
            println!("test_partial_string_slice");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);
                let slice = arc_string.slice(6..11); // "world"
                assert_eq!(slice.len(), 5);
                assert_eq!(slice.as_str(), "world");
                assert_eq!(slice.to_string(), "world".to_string());
                assert_eq!(&slice[..], "world");
            }

            // 测试字符串切片的开始和结束位置
            // #[tokio::test]
            // async fn test_boundary_string_slice() {
            println!("test_boundary_string_slice");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);

                // 切片到字符串的第一个字符
                let slice_start = arc_string.slice(0..1);
                assert_eq!(slice_start.len(), 1);
                assert_eq!(slice_start.as_str(), "h");

                // 切片到字符串的最后一个字符
                let slice_end = arc_string.slice(10..11);
                assert_eq!(slice_end.len(), 1);
                assert_eq!(slice_end.as_str(), "d");
            }

            // 测试字符串切片的越界情况
            // #[tokio::test]
            // async fn test_out_of_bound_string_slice() {
            println!("test_out_of_bound_string_slice");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);

                //___disable___
                // 尝试切片超出范围
                // let slice_out_of_bounds = arc_string.slice(10..20);
                // assert_eq!(slice_out_of_bounds.len(), 0);
                // assert_eq!(slice_out_of_bounds.as_str(), "");

                // 切片空字符串
                let empty_slice = arc_string.slice(5..5);
                assert_eq!(empty_slice.len(), 0);
                assert_eq!(empty_slice.as_str(), "");
            }

            // 测试克隆行为，确保 ArcString 是共享数据
            // #[tokio::test]
            // async fn test_cloning() {
            println!("test_cloning");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);
                let arc_string_clone = arc_string.clone();

                // 克隆后应该共享相同的数据
                assert_eq!(arc_string.len(), arc_string_clone.len());
                assert_eq!(arc_string.as_str(), arc_string_clone.as_str());

                // 确保是同一 Arc 实例
                assert!(Arc::ptr_eq(
                    &arc_string.ctx.data,
                    &arc_string_clone.ctx.data
                ));
            }

            // 测试不可变性，确保原始字符串不可修改
            // #[tokio::test]
            // async fn test_immutable() {
            println!("test_immutable");
            {
                let str = "immutable string";
                let arc_string = ArcString::from(str);

                // arc_string 应该是不可变的
                assert_eq!(arc_string.as_str(), str);

                // 尝试修改原始数据会失败
                // arc_string.push_str("new data"); // 这行代码应该报错，因为 ArcString 是不可变的
            }

            // 测试序列化与反序列化
            #[derive(Serialize, Deserialize, Debug, PartialEq)]
            struct TestStruct {
                #[serde(rename = "custom_string")]
                custom_string: ArcString,
            }

            // #[tokio::test]
            // async fn test_serialization() {
            println!("test_serialization");
            {
                let str = "hello world";
                let arc_string = ArcString::from(str);
                let test_struct = TestStruct {
                    custom_string: arc_string,
                };

                // 序列化
                let serialized = serde_json::to_string(&test_struct).unwrap();
                println!("Serialized: {}", serialized);

                // 反序列化
                let deserialized: TestStruct = serde_json::from_str(&serialized).unwrap();
                assert_eq!(deserialized.custom_string, test_struct.custom_string);
            }

            // 测试对大字符串的支持
            // #[tokio::test]
            // async fn test_large_string() {
            println!("test_large_string");
            {
                let large_str = "a".repeat(1_000_000); // 100万字符
                let arc_string = ArcString::from(large_str.clone());
                assert_eq!(arc_string.len(), large_str.len());
                assert_eq!(arc_string.as_str(), large_str);
                assert_eq!(arc_string.to_string(), large_str);
            }

            // 测试对大范围切片的支持
            // #[tokio::test]
            // async fn test_large_slice() {
            println!("test_large_slice");
            {
                let large_str = "a".repeat(1_000_000); // 100万字符
                let arc_string = ArcString::from(large_str.clone());

                let slice = arc_string.slice(100_000..200_000); // 切片 100,000 到 200,000
                assert_eq!(slice.len(), 100_000);
                assert_eq!(slice.as_str(), "a".repeat(100_000));
            }
            // #[tokio::test]
            // async fn test_hash_map_with_arc_string() {
            println!("test_hash_map_with_arc_string");
            {
                let mut map: HashMap<ArcString, u32> = HashMap::new();

                let arc_string1 = ArcString::from("key1");
                let arc_string2 = ArcString::from("key2");
                let arc_string3 = ArcString::from("key1");

                // 插入数据
                map.insert(arc_string1.clone(), 10);
                map.insert(arc_string2.clone(), 20);

                // 测试能否正确查找
                assert_eq!(*map.get(&arc_string1).unwrap(), 10);
                assert_eq!(*map.get(&arc_string2).unwrap(), 20);

                // 测试 `key1` 重复插入（应覆盖旧值）
                map.insert(arc_string3, 30);
                assert_eq!(*map.get(&arc_string1).unwrap(), 30);
            }
        }
    }
}

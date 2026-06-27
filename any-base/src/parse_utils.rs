use std::io;

/*
支持流量:
let value = "1"  得到 1值
let value = "1.3"  得到 1值
let value = "10"  得到 10值
let value = "10.2"  得到 10值
数字得到数字去掉小数点


let value = "1b"  得到 1值
let value = "1.3b"  得到 1值
let value = "10b"  得到 10值
let value = "10.2b"  得到 10值
最小是b 忽略小数点或就是*1


let value = "1k"  得到 1 * 1024值
let value = "1K"  得到 1 * 1024值
let value = "1.2k"  得到 1.2 * 1024值
let value = "1.2K"  得到 1.2 * 1024值


let value = "1m"  得到 1 * 1024 * 1024 值字节
let value = "1M"  得到 1 * 1024 * 1024 值字节
let value = "1.5m"  得到 1.5 * 1024 * 1024 值字节
let value = "1.7M"  得到 1.7 * 1024 * 1024 值字节




let value = "1g"  得到 1 * 1024 * 1024 * 1024 值字节
let value = "1G"  得到 1 * 1024 * 1024  * 1024值字节
let value = "1.1g"  得到 1.1 * 1024 * 1024 * 1024 值字节
let value = "1.2G"  得到 1.2 * 1024 * 1024  * 1024值字节



规律是没有单位就得到数字去除小数点, 不然就换算单位,  b或B忽略小数点







支持时间:
 let  value = "1" 得到  1
 let  value = "10" 得到  10
 let  value = "1.3" 得到  1
 let  value = "10.3" 得到  10
数字得到数字去掉小数点或 就是*1


 let  value = "1ms" 得到  1
 let  value = "1.2ms" 得到  1
 let  value = "1.20ms" 得到  1
最小msg, 忽略小数点


 let  value = "1s" 得到  1
 let  value = "1.2s" 得到  1.2 * 1000
 let  value = "1.22s" 得到  1.22 * 1000


 let  value = "1min" 得到  1 * 60
 let  value = "1.0min" 得到  1 * 60 * 1000
 let  value = "1.33min" 得到  1.33 * 60 * 1000

 let  value = "1h" 得到  1 * 60 * 60
 let  value = "1.0h" 得到  1 * 60 * 60 * 100

规律是没有单位就得到数字去除小数点,  不然就换算单位, 浮点数要*1000变成毫秒, 不是浮点数不要*1000就是秒,  ms忽略小数点

*/
pub struct ValueParser;

impl ValueParser {
    /// 统一解析入口：自动根据后缀识别是【流量】还是【时间】，并应用特定的浮点/整数换算规则
    pub fn parse(input: &str) -> io::Result<u64> {
        let input = input.trim();
        if input.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "input is empty",
            ));
        }

        // ====== 补丁：坚决拒绝任何负数配置 ======
        if input.starts_with('-') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "negative values are not allowed",
            ));
        }

        // 1. 分离数字和单位
        let (num_str, unit_str) = Self::split_num_and_unit(input);
        let unit_lower = unit_str.to_lowercase();

        // 2. 根据单位后缀，自动智能路由
        match unit_lower.as_str() {
            // ==================== 流量/字节单位 (b, k, m, g) ====================
            // 没有单位，或者明确是字节单位 'b' -> 直接去掉小数点后面的数
            "" | "b" => Self::parse_as_truncated_integer(num_str),

            "k" => Self::parse_as_float_with_multiplier(num_str, 1024.0),
            "m" => Self::parse_as_float_with_multiplier(num_str, 1024.0 * 1024.0),
            "g" => Self::parse_as_float_with_multiplier(num_str, 1024.0 * 1024.0 * 1024.0),

            // ==================== 时间单位 (ms, s, min, h) ====================
            // ms 始终忽略小数点
            "ms" => Self::parse_as_truncated_integer(num_str),

            "s" => {
                // 规律：是浮点数才 * 1000 变毫秒，纯整数不 * 1000
                if num_str.contains('.') {
                    Self::parse_as_float_with_multiplier(num_str, 1000.0)
                } else {
                    Self::parse_as_truncated_integer(num_str)
                }
            }
            "min" => {
                if num_str.contains('.') {
                    Self::parse_as_float_with_multiplier(num_str, 60.0 * 1000.0)
                } else {
                    // 非浮点数：比如 "1min" 得到 1 * 60 = 60
                    Self::parse_as_float_with_multiplier(num_str, 60.0)
                }
            }
            "h" => {
                if num_str.contains('.') {
                    Self::parse_as_float_with_multiplier(num_str, 60.0 * 60.0 * 1000.0)
                } else {
                    // 非浮点数：比如 "1h" 得到 1 * 60 * 60 = 3600
                    Self::parse_as_float_with_multiplier(num_str, 60.0 * 60.0)
                }
            }

            // 未知单位报错
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown configuration unit: '{}'", unit_str),
            )),
        }
    }

    /// 辅助函数：分离数字部分和单位部分
    fn split_num_and_unit(input: &str) -> (&str, &str) {
        let mid = input
            .find(|c: char| !c.is_numeric() && c != '.' && c != '-')
            .unwrap_or(input.len());
        let num_part = input[..mid].trim();
        let unit_part = input[mid..].trim();
        (num_part, unit_part)
    }

    /// 辅助函数：针对无单位、b、ms，或者非浮点时间，直接截断小数点解析为整数
    /// 辅助函数：针对无单位、b、ms，直接截断小数点解析为整数
    fn parse_as_truncated_integer(num_str: &str) -> io::Result<u64> {
        let target_str = if let Some(idx) = num_str.find('.') {
            &num_str[..idx]
        } else {
            num_str
        };

        // 如果截断后为空（例如输入的是 ".5" 或 "."），且原字符串不只是一个 "."
        if target_str.is_empty() {
            if num_str == "." {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid number: '.'",
                ));
            }
            // ".5" 没有单位或带 b/ms 时，整数部分视为 0
            return Ok(0);
        }

        target_str.parse::<u64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid integer format: {}", e),
            )
        })
    }

    /// 辅助函数：针对换算乘积逻辑（支持浮点数和整数乘系数）
    fn parse_as_float_with_multiplier(num_str: &str, multiplier: f64) -> io::Result<u64> {
        let val = num_str.parse::<f64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid float format: {}", e),
            )
        })?;

        Ok((val * multiplier) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // 1. 流量/字节 (Bytes) 单元测试
    // =========================================================================
    #[test]
    fn test_parse_bytes_no_unit() {
        // 规律：没有单位就得到数字去除小数点
        assert_eq!(ValueParser::parse("1").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.3").unwrap(), 1);
        assert_eq!(ValueParser::parse("10").unwrap(), 10);
        assert_eq!(ValueParser::parse("10.2").unwrap(), 10);
    }

    #[test]
    fn test_parse_bytes_b_unit() {
        // 规律：最小是 b 忽略小数点或就是 * 1
        assert_eq!(ValueParser::parse("1b").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.3b").unwrap(), 1);
        assert_eq!(ValueParser::parse("10b").unwrap(), 10);
        assert_eq!(ValueParser::parse("10.2b").unwrap(), 10);

        // 支持大写 B
        assert_eq!(ValueParser::parse("1B").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.3B").unwrap(), 1);
        assert_eq!(ValueParser::parse("10B").unwrap(), 10);
        assert_eq!(ValueParser::parse("10.2B").unwrap(), 10);
    }

    #[test]
    fn test_parse_bytes_k_unit() {
        // 规律：K/k 单位得到 对应值 * 1024
        assert_eq!(ValueParser::parse("1k").unwrap(), 1 * 1024);
        assert_eq!(ValueParser::parse("1K").unwrap(), 1 * 1024);
        assert_eq!(ValueParser::parse("1.2k").unwrap(), (1.2 * 1024.0) as u64); // 1228
        assert_eq!(ValueParser::parse("1.2K").unwrap(), (1.2 * 1024.0) as u64); // 1228
    }

    #[test]
    fn test_parse_bytes_m_unit() {
        // 规律：M/m 单位得到 对应值 * 1024 * 1024
        assert_eq!(ValueParser::parse("1m").unwrap(), 1 * 1024 * 1024);
        assert_eq!(ValueParser::parse("1M").unwrap(), 1 * 1024 * 1024);
        assert_eq!(
            ValueParser::parse("1.5m").unwrap(),
            (1.5 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(
            ValueParser::parse("1.7M").unwrap(),
            (1.7 * 1024.0 * 1024.0) as u64
        );
    }

    #[test]
    fn test_parse_bytes_g_unit() {
        // 规律：G/g 单位得到 对应值 * 1024 * 1024 * 1024
        assert_eq!(ValueParser::parse("1g").unwrap(), 1 * 1024 * 1024 * 1024);
        assert_eq!(ValueParser::parse("1G").unwrap(), 1 * 1024 * 1024 * 1024);
        assert_eq!(
            ValueParser::parse("1.1g").unwrap(),
            (1.1 * 1024.0 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(
            ValueParser::parse("1.2G").unwrap(),
            (1.2 * 1024.0 * 1024.0 * 1024.0) as u64
        );
    }

    // =========================================================================
    // 2. 时间 (Time) 单元测试
    // =========================================================================
    #[test]
    fn test_parse_time_no_unit() {
        // 规律：没有单位得到数字去掉小数点
        assert_eq!(ValueParser::parse("1").unwrap(), 1);
        assert_eq!(ValueParser::parse("10").unwrap(), 10);
        assert_eq!(ValueParser::parse("1.3").unwrap(), 1);
        assert_eq!(ValueParser::parse("10.3").unwrap(), 10);
    }

    #[test]
    fn test_parse_time_ms_unit() {
        // 规律：最小 ms, 忽略小数点
        assert_eq!(ValueParser::parse("1ms").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.2ms").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.20ms").unwrap(), 1);

        // 支持大写 MS
        assert_eq!(ValueParser::parse("1MS").unwrap(), 1);
        assert_eq!(ValueParser::parse("1.5mS").unwrap(), 1);
    }

    #[test]
    fn test_parse_time_s_unit() {
        // 规律：非浮点数不要 * 1000（即纯数字截断），浮点数才 * 1000
        assert_eq!(ValueParser::parse("1s").unwrap(), 1);
        assert_eq!(ValueParser::parse("10s").unwrap(), 10);
        assert_eq!(ValueParser::parse("1.2s").unwrap(), 1200); // 1.2 * 1000
        assert_eq!(ValueParser::parse("1.22s").unwrap(), 1220); // 1.22 * 1000

        // 大写 S
        assert_eq!(ValueParser::parse("2S").unwrap(), 2);
        assert_eq!(ValueParser::parse("1.5S").unwrap(), 1500);
    }

    #[test]
    fn test_parse_time_min_unit() {
        // 规律：非浮点数 * 60，浮点数 * 60 * 1000
        assert_eq!(ValueParser::parse("1min").unwrap(), 1 * 60);
        assert_eq!(ValueParser::parse("10min").unwrap(), 10 * 60);
        assert_eq!(ValueParser::parse("1.0min").unwrap(), 1 * 60 * 1000);
        assert_eq!(
            ValueParser::parse("1.33min").unwrap(),
            (1.33 * 60.0 * 1000.0) as u64
        );

        // 大写 MIN / Min
        assert_eq!(ValueParser::parse("2MIN").unwrap(), 2 * 60);
        assert_eq!(
            ValueParser::parse("1.5Min").unwrap(),
            (1.5 * 60.0 * 1000.0) as u64
        );
    }

    #[test]
    fn test_parse_time_h_unit() {
        // 规律：非浮点数 * 60 * 60，浮点数 * 60 * 60 * 1000
        assert_eq!(ValueParser::parse("1h").unwrap(), 1 * 60 * 60);
        assert_eq!(ValueParser::parse("2h").unwrap(), 2 * 60 * 60);
        assert_eq!(ValueParser::parse("1.0h").unwrap(), 1 * 60 * 60 * 1000);
        assert_eq!(
            ValueParser::parse("2.5h").unwrap(),
            (2.5 * 60.0 * 60.0 * 1000.0) as u64
        );

        // 大写 H
        assert_eq!(ValueParser::parse("1H").unwrap(), 1 * 60 * 60);
        assert_eq!(
            ValueParser::parse("1.2H").unwrap(),
            (1.2 * 60.0 * 60.0 * 1000.0) as u64
        );
    }

    // =========================================================================
    // 3. 容错性与极端边界测试 (Robustness & Edge Cases)
    // =========================================================================
    #[test]
    fn test_parse_spaces_handling() {
        // 测试带有前后空格、中间空格的非规范输入
        assert_eq!(ValueParser::parse("  5m  ").unwrap(), 5 * 1024 * 1024);
        assert_eq!(
            ValueParser::parse("1.5   M").unwrap(),
            (1.5 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(ValueParser::parse("\t 10.5 s \n").unwrap(), 10500);
    }

    #[test]
    fn test_parse_invalid_inputs() {
        // 空字符串报错
        assert!(ValueParser::parse("").is_err());
        assert!(ValueParser::parse("   ").is_err());

        // 纯字母/无有效数字报错
        assert!(ValueParser::parse("abc").is_err());
        assert!(ValueParser::parse("m").is_err());

        // 未知/不支持的非法单位后缀报错
        assert!(ValueParser::parse("100xyz").is_err());
        assert!(ValueParser::parse("5day").is_err());

        // 多个小数点的错误语法报错
        assert!(ValueParser::parse("1.2.3M").is_err());
    }

    // =========================================================================
    // 4. 补充测试：零值、大数、浮点边界、非法数字
    // =========================================================================
    #[test]
    fn test_parse_zero_values() {
        // 无单位 / b：截断小数
        assert_eq!(ValueParser::parse("0").unwrap(), 0);
        assert_eq!(ValueParser::parse("0.9").unwrap(), 0);
        assert_eq!(ValueParser::parse("0b").unwrap(), 0);
        assert_eq!(ValueParser::parse("0.5B").unwrap(), 0);

        // 流量单位
        assert_eq!(ValueParser::parse("0k").unwrap(), 0);
        assert_eq!(ValueParser::parse("0.5k").unwrap(), (0.5 * 1024.0) as u64);
        assert_eq!(ValueParser::parse("0m").unwrap(), 0);
        assert_eq!(ValueParser::parse("0g").unwrap(), 0);

        // 时间单位
        assert_eq!(ValueParser::parse("0ms").unwrap(), 0);
        assert_eq!(ValueParser::parse("0.9ms").unwrap(), 0);
        assert_eq!(ValueParser::parse("0s").unwrap(), 0);
        assert_eq!(ValueParser::parse("0.5s").unwrap(), 500);
        assert_eq!(ValueParser::parse("0min").unwrap(), 0);
        assert_eq!(
            ValueParser::parse("0.5min").unwrap(),
            (0.5 * 60.0 * 1000.0) as u64
        );
        assert_eq!(ValueParser::parse("0h").unwrap(), 0);
        assert_eq!(
            ValueParser::parse("0.5h").unwrap(),
            (0.5 * 60.0 * 60.0 * 1000.0) as u64
        );
    }

    #[test]
    fn test_parse_large_values() {
        assert_eq!(ValueParser::parse("1024k").unwrap(), 1024 * 1024);
        assert_eq!(ValueParser::parse("1024K").unwrap(), 1024 * 1024);
        assert_eq!(ValueParser::parse("999m").unwrap(), 999 * 1024 * 1024);
        assert_eq!(ValueParser::parse("2g").unwrap(), 2 * 1024 * 1024 * 1024);
        assert_eq!(ValueParser::parse("100ms").unwrap(), 100);
        assert_eq!(ValueParser::parse("3600s").unwrap(), 3600);
        assert_eq!(ValueParser::parse("1440min").unwrap(), 1440 * 60);
        assert_eq!(ValueParser::parse("24h").unwrap(), 24 * 60 * 60);
    }

    #[test]
    fn test_parse_float_dot_zero_suffix() {
        // 带 ".0" 仍视为浮点数，走 * 1000 换算
        assert_eq!(ValueParser::parse("1.0s").unwrap(), 1000);
        assert_eq!(ValueParser::parse("10.0s").unwrap(), 10000);
        assert_eq!(ValueParser::parse("2.0S").unwrap(), 2000);
    }

    #[test]
    fn test_parse_bytes_fractional_edge_cases() {
        assert_eq!(ValueParser::parse("0.9k").unwrap(), (0.9 * 1024.0) as u64);
        assert_eq!(ValueParser::parse("2.5k").unwrap(), (2.5 * 1024.0) as u64);
        assert_eq!(
            ValueParser::parse("0.1m").unwrap(),
            (0.1 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(
            ValueParser::parse("0.01g").unwrap(),
            (0.01 * 1024.0 * 1024.0 * 1024.0) as u64
        );
    }

    #[test]
    fn test_parse_time_ms_truncation() {
        assert_eq!(ValueParser::parse("10.5ms").unwrap(), 10);
        assert_eq!(ValueParser::parse("99.99ms").unwrap(), 99);
        assert_eq!(ValueParser::parse("0.1ms").unwrap(), 0);
    }

    #[test]
    fn test_parse_time_s_mixed_integer_and_float() {
        assert_eq!(ValueParser::parse("0.1s").unwrap(), 100);
        assert_eq!(ValueParser::parse("0.01s").unwrap(), 10);
        assert_eq!(ValueParser::parse("100.5s").unwrap(), 100500);
    }

    #[test]
    fn test_parse_time_min_h_integer_only() {
        assert_eq!(ValueParser::parse("5min").unwrap(), 5 * 60);
        assert_eq!(ValueParser::parse("3h").unwrap(), 3 * 60 * 60);
        assert_eq!(ValueParser::parse("2.0min").unwrap(), 2 * 60 * 1000);
        assert_eq!(
            ValueParser::parse("1.5h").unwrap(),
            (1.5 * 60.0 * 60.0 * 1000.0) as u64
        );
    }

    #[test]
    fn test_parse_unit_case_mixtures() {
        // 单位大小写不敏感（合法后缀）
        assert_eq!(ValueParser::parse("500Ms").unwrap(), 500);
        assert_eq!(ValueParser::parse("1Min").unwrap(), 60);
        assert_eq!(ValueParser::parse("2.5K").unwrap(), (2.5 * 1024.0) as u64);
        assert_eq!(ValueParser::parse("10G").unwrap(), 10 * 1024 * 1024 * 1024);
        assert_eq!(ValueParser::parse("5M").unwrap(), 5 * 1024 * 1024);
        assert_eq!(ValueParser::parse("3H").unwrap(), 3 * 60 * 60);
    }

    #[test]
    fn test_parse_unknown_and_malformed_units() {
        assert!(ValueParser::parse("1Hr").is_err());
        assert!(ValueParser::parse("1hr").is_err());
        assert!(ValueParser::parse("1d").is_err());
        assert!(ValueParser::parse("1sec").is_err());
        assert!(ValueParser::parse("1kb").is_err()); // "kb" != "k"
        assert!(ValueParser::parse("1.5.0m").is_err());
    }

    #[test]
    fn test_parse_invalid_numeric_formats() {
        assert_eq!(ValueParser::parse(".5k").unwrap(), (0.5 * 1024.0) as u64);
        assert_eq!(ValueParser::parse(".5").unwrap(), 0);
        assert!(ValueParser::parse("k5").is_err());
        assert!(ValueParser::parse("-1").is_err());
        assert!(ValueParser::parse("-1.5k").is_err());
        assert!(ValueParser::parse("1e3k").is_err());
        assert!(ValueParser::parse("+1").is_err());
    }

    #[test]
    fn test_parse_leading_zeros() {
        assert_eq!(ValueParser::parse("01").unwrap(), 1);
        assert_eq!(ValueParser::parse("01.5k").unwrap(), (1.5 * 1024.0) as u64);
        assert_eq!(ValueParser::parse("007ms").unwrap(), 7);
    }
}

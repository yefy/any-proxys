#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use serde::Deserialize;
use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection, WMIDateTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let com_con = COMLibrary::new()?;
    let wmi_con = WMIConnection::new(com_con.into())?;

    let results: Vec<HashMap<String, Variant>> =
        wmi_con.raw_query("SELECT * FROM Win32_OperatingSystem")?;

    for os in results {
        println!("{:#?}", os);
    }

    #[derive(Deserialize, Debug)]
    struct Win32_OperatingSystem {
        Caption: String,
        Name: String,
        CurrentTimeZone: i16,
        Debug: bool,
        EncryptionLevel: u32,
        ForegroundApplicationBoost: u8,
        LastBootUpTime: WMIDateTime,
    }

    let results: Vec<Win32_OperatingSystem> = wmi_con.query()?;

    for os in results {
        println!("{:#?}", os);
    }

    let results: Vec<HashMap<String, Variant>> = wmi_con.raw_query(
        "SELECT Name, PercentDiskTime FROM Win32_PerfFormattedData_PerfDisk_PhysicalDisk",
    )?;

    for os in results {
        println!("{:#?}", os);
    }

    Ok(())
}

/*
use wmi::{COMError, WMIConnection};
use std::error::Error;

fn main11() -> Result<(), Box<dyn Error>> {
    let wmi = WMIConnection::new(COMError::default())?;

    // 查询磁盘性能信息
    let results = wmi.query("SELECT Name, DiskReadBytesPersec, DiskWriteBytesPersec FROM Win32_PerfFormattedData_PerfDisk_PhysicalDisk")?;

    for result in results {
        let name: String = result.get("Name")?;
        let read_bytes: u64 = result.get("DiskReadBytesPersec")?;
        let write_bytes: u64 = result.get("DiskWriteBytesPersec")?;

        let total_io = read_bytes + write_bytes;
        let io_percentage = if total_io > 0 {
            (read_bytes as f64 / total_io as f64) * 100.0
        } else {
            0.0
        };

        println!("Drive: {} - IO Percentage: {:.2}%", name, io_percentage);
    }

    Ok(())
}

 */

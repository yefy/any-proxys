use futures_util::SinkExt;
use std::error::Error;
use tokio::sync::mpsc;

pub async fn rx_close() {
    let tx = {
        let (tx, mut rx) = mpsc::channel(1);

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            println!("rx close");
            rx.close();
            rx.close();
            //rx.recv().await;
        });
        tx
    };

    loop {
        println!("tx send");
        if let Err(e) = tx.send(()).await {
            println!("tx send e:{}", e);
        }
    }
}

pub async fn tx_close() {
    let mut rx = {
        let (tx, mut rx) = mpsc::channel::<()>(1);

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            println!("tx closed 111");
            //tx.send(()).await;
            //tx.closed().await;
            std::mem::drop(tx);
            //tx.closed();
            println!("tx closed 222");
            tokio::time::sleep(tokio::time::Duration::from_secs(500000)).await;
        });
        rx
    };

    loop {
        println!("rx recv");
        if rx.recv().await.is_none() {
            println!("rx recv is_none");
        }
    }
}

pub async fn async_channel() {
    let (tx, rx) = async_channel::unbounded();
    let rx2 = rx.clone();

    tokio::spawn(async move {
        loop {
            let n = rx.recv().await.unwrap();
            println!("rx:{}", n);
        }
    });

    tokio::spawn(async move {
        loop {
            let n = rx2.recv().await.unwrap();
            println!("rx2:{}", n);
        }
    });

    for n in 0..10 {
        //println!("tx:{}", n);
        tx.send(n).await;
    }
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}

pub async fn futures_channel() {
    use futures_util::StreamExt;
    let (mut tx, mut rx) = futures::channel::mpsc::channel(111);

    tokio::spawn(async move {
        loop {
            let n = rx.next().await.unwrap();
            println!("rx:{}", n);
        }
    });

    for n in 0..10 {
        //println!("tx:{}", n);
        tx.send(n).await;
    }
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    //async_channel().await;
    //rx_close().await;
    tx_close().await;
    Ok(())
}

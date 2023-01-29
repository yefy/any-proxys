use any_tunnel2::anychannel;
use any_tunnel2::anychannel::AnyAsyncReceiver;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    {
        let mut anychannel =
            anychannel::AnyAsyncChannelMap::<u32, anychannel::AnyChannel<u32>>::new();
        let mut rx = anychannel.channel(10, 1);
        anychannel.send(1, 3).await;
        let buf = rx.recv().await;
        //buf == Some(3)
    }

    {
        let mut anychannel =
            anychannel::AnyAsyncChannelMap::<u32, anychannel::AnyUnboundedChannel<u32>>::new();
        let mut rx = anychannel.channel(10, 1);
        anychannel.send(1, 3).await;
        let buf = rx.recv().await;
        //buf == Some(3)
    }

    {
        let mut anychannel =
            anychannel::AnyAsyncChannelRoundMap::<anychannel::AnyChannel<u32>>::new();
        let (key, mut rx) = anychannel.channel(10);
        anychannel.send(3).await;
        let buf = rx.recv().await;
        //buf == Some(3)
    }

    {
        let mut anychannel =
            anychannel::AnyAsyncChannelRoundMap::<anychannel::AnyUnboundedChannel<u32>>::new();
        let (key, mut rx) = anychannel.channel(10);
        anychannel.send(3).await;
        let buf = rx.recv().await;
        //buf == Some(3)
    }
    Ok(())
}

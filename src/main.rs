use std::time::Duration;



#[tokio::main(flavor = "multi_thread")]
async fn main() {
    println!("Starting payment service");
    loop {
        
        tokio::time::sleep(Duration::from_mins(5)).await;
    }
}

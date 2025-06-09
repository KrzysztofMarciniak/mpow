mod html;
mod jwt;
mod routing;
mod values;

#[tokio::main]
async fn main() {
	routing::start_server().await;
}

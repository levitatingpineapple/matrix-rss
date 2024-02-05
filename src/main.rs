use actix_web::{
	http, 
	web::Data, 
	HttpServer, App, HttpResponse, get
};
use clap::{
	command, 
	Parser, 
	arg
};
use handlebars::*;
use matrix_sdk::{
	config::SyncSettings, 
	ruma::{
		events::room::message::MessageType, 
		RoomId
	},
	Client, 
	Room
};
use ::rss::*;
use chrono::*;

pub mod html;
use html::*;

pub mod matrix;
use matrix::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// Bind address
	#[arg(long, default_value = "localhost")]
	bind: String,

	/// Port to listen on
	#[arg(long, default_value_t = 8080)]
	port: u16,
	
	/// Matrix user ID
	#[arg(long)]
	mxid: String,
	
	/// Matrix password
	#[arg(long)]
	pass: String,
	
	/// Matrix room name
	#[arg(long)]
	room: String
}

struct AppState {
	client: Client,
	handlebars: Handlebars<'static>,
	room: Room
}

impl AppState {
	async fn new(args: &Args) -> Self {
		let client = matrix::client(&args.mxid, &args.pass).await;
		client.sync_once(SyncSettings::default()).await.expect("Sync failed!");
		println!("Synced! ✅");
		let mut handlebars = Handlebars::new();
		handlebars.register_template_string("feed", include_str!("feed.hbs")).unwrap();
		AppState {
			client: client.clone(),
			handlebars: handlebars,
			room: client.get_room(
				&RoomId::parse(&args.room)
					.expect("Invalid room ID!")
			).expect("Room not found!")
		}
	}
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let args: Args = Args::parse();
	let data = Data::new(
		AppState::new(&args).await
	);
	HttpServer::new(move || {
		App::new()
			.app_data(data.clone())
			.service(feed)
			.service(rss)
	})
	.bind((args.bind, args.port))?
	.run()
	.await
}

#[get("/")]
async fn feed(data: Data<AppState>) -> HttpResponse {
	#[derive(::serde::Serialize)]
	struct Feed {
		avatar: String,
		name: String,
		messages: String
	}
	
	HttpResponse::Ok().body(
		data.handlebars.render(
			"feed", 
			&Feed {
				avatar: data.room.avatar_url()
					.map(|mxc| url(&mxc)).unwrap_or(String::new()),
				name: name(&data.room).await
					.unwrap_or("Room".to_string()),
				messages: messages(&data.room).await.iter().map(|m| { 
					format!(
						"\t\t\t{}\n\t\t\t{}\n",
						m.content.msgtype.to_html(),
						m.origin_server_ts.to_html(),
					)
				}).collect::<String>()
			}
		).unwrap()
	)
}

#[get("/rss")]
async fn rss(data: Data<AppState>) -> HttpResponse {
	HttpResponse::Ok()
		.content_type(http::header::ContentType::xml())
		.body(
			ChannelBuilder::default()
			.title(name(&data.room).await.unwrap_or("Room".to_string()))
			.image(
				data.room
					.avatar_url()
					.map(|mxc| 
						ImageBuilder::default()
							.url(url(&mxc))
							.build()
					)
			)
			.link(&data.client.homeserver().to_string())
			.items(
				messages(&data.room).await.iter().map(|m| {
					let title = Some(match m.content.msgtype {
						MessageType::Audio(_) => "Audio",
						MessageType::Image(_) => "Image",
						MessageType::Text(_) => "Text",
						MessageType::Video(_) => "Video",
						_ => "Message",
					}.to_string());
					let guid = Some(Guid { 
						value: m.origin_server_ts.get().to_string(), 
						permalink: false
					});
					let content = Some(m.content.clone().msgtype.to_html());
					let pub_date = Some(
						DateTime::<Local>::from(
							m.origin_server_ts
								.to_system_time()
								.unwrap()
						).to_rfc2822()
					);
					ItemBuilder::default()
						.title(title)
						.guid(guid)
						.content(content)
						.pub_date(pub_date)
						.build()
				})
				.collect::<Vec<Item>>()
			)
			.build()
			.to_string()
		)
}
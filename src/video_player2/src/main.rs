//use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use actix_web::{web, App, HttpServer, Result};
use actix_files as fs;

/*async fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(r#"
            <video width="640" height="480" controls>
                <source src="/video" type="video/mp4">
                Your Browser does not support the video tag
            </video>
        "#)
}*/

async fn index() -> Result<fs::NamedFile> {
    Ok(fs::NamedFile::open("src/index.html")?)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .service(fs::Files::new("/static", "./static"))
            //.service(fs::Files::new("/video", ".").index_file("video.mp4"))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

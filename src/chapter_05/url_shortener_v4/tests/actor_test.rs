use url_shortener_v4::actor::contador::ContadorSharded;
use url_shortener_v4::domain::url_entry::UrlCode;

#[tokio::test]
async fn actor_sharded_contadores_correctos() {
    let contador = ContadorSharded::iniciar(4);
    let code_a = UrlCode::parse("url-a").unwrap();
    let code_b = UrlCode::parse("url-b").unwrap();

    // Incrementar desde múltiples tareas concurrentes
    let mut tareas = Vec::new();
    for _ in 0..10 {
        let c = contador.clone();
        let ca = code_a.clone();
        let cb = code_b.clone();
        tareas.push(tokio::spawn(async move {
            c.incrementar(ca).await;
            c.incrementar(cb.clone()).await;
            c.incrementar(cb).await;
        }));
    }
    for t in tareas {
        t.await.unwrap();
    }

    // Cada mensaje de un código va al mismo actor, en orden: al llegar
    // `obtener`, todos los `incrementar` anteriores ya se procesaron.
    assert_eq!(contador.obtener(&code_a).await, 10);
    assert_eq!(contador.obtener(&code_b).await, 20);
}

#[tokio::test]
async fn snapshot_agrega_todos_los_shards() {
    let contador = ContadorSharded::iniciar(8);
    let codes: Vec<_> = (0..8)
        .map(|i| UrlCode::parse(format!("url-{i}")).unwrap())
        .collect();

    for code in &codes {
        contador.incrementar(code.clone()).await;
        contador.incrementar(code.clone()).await;
    }

    let snap = contador.snapshot().await;
    assert_eq!(snap.len(), 8);
    assert_eq!(snap.values().sum::<u64>(), 16); // 8 URLs × 2 clicks cada una
}

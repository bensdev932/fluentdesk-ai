//! Overall health and pinned-provider view calculations for `TutorRouter`.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    router_types::{ProviderDescriptorView, RouterHealthView},
};

pub(crate) fn try_cloud_overall_health(
    cloud_opt_in: bool,
    preferred: ProviderPreference,
    list: &[ProviderDescriptorView],
) -> Option<RouterHealthView> {
    if let Some(pref) = preferred.as_pinned() {
        return pref
            .is_cloud()
            .then(|| pinned_health_view(pref, cloud_opt_in, list));
    }
    if !cloud_opt_in {
        return None;
    }
    list.iter()
        .find(|p| p.is_cloud && p.available)
        .map(|p| health_view("available", p.id, Some(p.selected_model.clone()), None))
}

pub(crate) fn ollama_overall_health(ol: &ProviderDescriptorView) -> RouterHealthView {
    if ol.available {
        health_view(
            "available",
            ProviderId::Ollama,
            Some(ol.selected_model.clone()),
            None,
        )
    } else {
        health_view(
            "unavailable",
            ProviderId::Ollama,
            None,
            ol.status_detail.clone(),
        )
    }
}

fn pinned_health_view(
    pref: ProviderId,
    cloud_opt_in: bool,
    list: &[ProviderDescriptorView],
) -> RouterHealthView {
    if pref.is_cloud() && !cloud_opt_in {
        return health_view(
            "unavailable",
            pref,
            None,
            Some("Cloud AI opt-in is disabled.".into()),
        );
    }
    match list.iter().find(|p| p.id == pref) {
        Some(p) if p.available => {
            health_view("available", pref, Some(p.selected_model.clone()), None)
        }
        Some(p) if pref.is_cloud() && !p.configured => {
            let var = pref.env_var_name().unwrap_or("API_KEY");
            let msg = format!("{} API key is not configured ({var}).", pref.label());
            health_view("misconfigured", pref, None, Some(msg))
        }
        Some(p) => health_view("unavailable", pref, None, p.status_detail.clone()),
        None => health_view("unavailable", pref, None, None),
    }
}

fn health_view(
    status: &'static str,
    provider: ProviderId,
    model: Option<String>,
    error: Option<String>,
) -> RouterHealthView {
    RouterHealthView {
        status,
        provider,
        is_cloud: provider.is_cloud(),
        model,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ollama::OllamaConfig, router::TutorRouter, router_types::RouterConfigPatch};
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn overall_health_skips_stalling_ollama_probe_when_cloud_is_ready() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let blackhole_port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_bg = Arc::clone(&hits);
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                hits_bg.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    drop(sock);
                });
            }
        });
        let router = TutorRouter::new(
            OllamaConfig::try_new("127.0.0.1", blackhole_port, "llama3.2:3b").unwrap(),
        );
        router
            .apply_config_patch(RouterConfigPatch {
                cloud_opt_in: Some(true),
                preferred_provider: Some(ProviderPreference::Auto),
                api_keys: HashMap::from([
                    (ProviderId::Groq, "gsk_fast".into()),
                    (ProviderId::Gemini, "AIza_fast".into()),
                ]),
                ..Default::default()
            })
            .await
            .unwrap();
        let t0 = Instant::now();
        let auto_health = router.overall_health().await;
        assert!(t0.elapsed() < Duration::from_millis(250));
        assert_eq!(
            (
                auto_health.status,
                auto_health.provider,
                auto_health.is_cloud
            ),
            ("available", ProviderId::Groq, true)
        );
        router
            .apply_config_patch(RouterConfigPatch {
                preferred_provider: Some(ProviderPreference::Gemini),
                ..Default::default()
            })
            .await
            .unwrap();
        let t1 = Instant::now();
        let pinned_health = router.overall_health().await;
        assert!(t1.elapsed() < Duration::from_millis(250));
        assert_eq!(
            (
                pinned_health.status,
                pinned_health.provider,
                pinned_health.is_cloud
            ),
            ("available", ProviderId::Gemini, true)
        );
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }
}

# Jamendo provider review

Reviewed **2026-10-27 (Asia/Singapore)** against official sources:

- [API introduction](https://developer.jamendo.com/v3.0/docs)
- [Tracks endpoint](https://developer.jamendo.com/v3.0/tracks)
- [Application credentials and access](https://developer.jamendo.com/v3.0/authentication)
- [Response codes](https://developer.jamendo.com/v3.0/response-codes)
- [API Terms of Use](https://devportal.jamendo.com/api_terms_of_use), page states last modified September 17, 2013.

The terms allow free non-commercial API use subject to each track's Creative Commons license. Commercial use needs a separate arrangement. Applications must credit the artist and Jamendo and link each track to its Jamendo page. Caching is limited to operational necessity; offline access is prohibited. These conditions are compatible with this personal-listening, streaming-only CLI design; they do not grant a general commercial music license. Terms may change.

## Credentials and requests

Register an application and supply your own `JAMENDO_CLIENT_ID`. No login/OAuth or client secret is used for public read requests. Do not commit, share or include credentials in diagnostics. The application registration description should point to this published repository. Each user must observe the provider's registration conditions; a bundled shared production ID is deliberately absent.

`GET https://api.jamendo.com/v3.0/tracks/` uses `client_id`, `format=json`, `search=<original text>`, `type=single albumtrack`, `limit=40`, `order=relevance`, `audioformat=mp31`. The documented `audio` field is the streaming URL; `audiodownload` and `/tracks/file?action=download` are never used. The response supplies name, artist, album, duration, `shareurl` and `license_ccurl`, so no resolution request is needed. HTTP errors and API code 6 (rate limit) fail immediately without retries.

The official authentication page asks applications expecting more than 500,000 hits to contact Jamendo; it gives no explicit time period or fixed requests-per-second entitlement. Account/provider restrictions still apply. Two calls per invocation is an application bound, not a rate-limit guarantee; repeated user invocations can still be limited. mzk does not circumvent restrictions.

## License and attribution handling

Candidates must have a recognized Creative Commons URL (BY, BY-SA, BY-ND, BY-NC, BY-NC-SA or BY-NC-ND; versions 1.0/2.0/2.5/3.0/4.0), a Jamendo track backlink and an HTTPS Jamendo streaming URL. Other licenses, localized license paths or CDN hostnames are rejected conservatively. This is metadata validation, not independent copyright clearance. Music remains owned/licensed by its rightsholders. A duration and URL do not prove playability; mpv must confirm loading.

Every successful play prints the supplied artist and title, a Jamendo credit with track link, and the per-track license link marked as an unmodified stream. mzk neither edits music nor offers a download, performance, synchronization or redistribution license. No logos are copied and Jamendo is not presented as this application's author. MIT applies only to first-party software and documentation.

## Privacy and practical limits

Queries and client ID are sent directly to the provider over TLS. Stream access exposes the listener's network address to Jamendo/CDN; mpv handles its network requests and transient memory buffers. No mzk server, telemetry, result cache, audio file or search history exists. Runtime JSON has no musical metadata. Provider-side collection and availability are outside mzk's control.

The catalog may lack a named composition or popular artist. Text is not interpreted semantically. The provider's own free-text search can use tags or similar artists internally; mzk only scores returned text fields. Unknown hosts are not scraped, extracted or resolved through unofficial services. No DRM or website parsing path exists.

Real service and playback evidence, including blocked conditions, is tracked separately in [VERIFICATION.md](VERIFICATION.md). Synthetic fixtures are explicitly labeled in source and do not represent recorded Jamendo responses.

The documentation's public read-only testing client was tried once on the review date and returned code 11 (Suspended Application). Do not rely on that example client for this application. No real catalog response or playback has been validated with a registered ID.

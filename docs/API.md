
### Edit Messages

**PUT /edit/:messageId** — Edit a sent message
```json
{ "body": "Updated message text" }
// Response: { "success": true }
```
Only messages sent by the bot (fromMe) can be edited.

### HTTP Forward Proxy

Use the NAS as a residential IP proxy for services that block datacenter IPs.

**POST /proxy** — Forward an HTTP request
```json
{
  "url": "https://target.com/api/endpoint",
  "method": "POST",
  "headers": { "Content-Type": "application/json" },
  "body": "{\"key\": \"value\"}",
  "cookies": "session=abc123",
  "timeout": 30000,
  "followRedirects": true,
  "maxRedirects": 10,
  "encoding": "utf8",
  "session": "my-session-id"
}
```
Response:
```json
{
  "status": 200,
  "headers": { "content-type": "application/json" },
  "cookies": ["session=xyz; Path=/; HttpOnly"],
  "body": "response body text or base64",
  "url": "https://final-url-after-redirects.com",
  "redirectCount": 0
}
```

**POST /proxy/session** — Manage cookie sessions
```json
// Create: { "action": "create", "session": "garmin-auth" }
// Get cookies: { "action": "cookies", "session": "garmin-auth" }
// Destroy: { "action": "destroy", "session": "garmin-auth" }
```

#### Proxy Features
- **All HTTP methods**: GET, POST, PUT, DELETE, PATCH, HEAD
- **Cookie sessions**: Persist cookies across multiple requests (SSO flows)
- **Redirect following**: Configurable with max redirect limit
- **Binary support**: Set `encoding: "base64"` for binary responses
- **Timeout control**: Per-request timeout (default 30s)
- **Custom headers**: Forward any headers to the target
- **Cookie forwarding**: Send and receive cookies

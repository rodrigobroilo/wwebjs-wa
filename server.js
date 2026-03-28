const express = require('express');
const { Client, LocalAuth, MessageMedia, Poll } = require('whatsapp-web.js');
const qrcode = require('qrcode');

const app = express();
// Explicit UTF-8 charset for proper emoji/accents support
app.use(express.json({ limit: '50mb' }));
app.use(express.urlencoded({ extended: true, limit: '50mb' }));
app.use((req, res, next) => {
  res.setHeader('Content-Type', 'application/json; charset=utf-8');
  next();
});

const PORT = process.env.PORT || 3100;
const API_KEY = process.env.API_KEY || 'change-me';

// Message storage (in-memory ring buffer)
const MAX_MESSAGES = parseInt(process.env.MAX_MESSAGES) || 1000;
const messages = [];

let qrData = null;
let isReady = false;
let clientInfo = null;

const client = new Client({
  authStrategy: new LocalAuth({ dataPath: '/data/session' }),
  puppeteer: {
    headless: true,
    args: [
      '--no-sandbox',
      '--disable-setuid-sandbox',
      '--disable-dev-shm-usage',
      '--disable-gpu',
      '--single-process'
    ],
    executablePath: process.env.PUPPETEER_EXECUTABLE_PATH || undefined
  }
});

// ==================== CLIENT EVENTS ====================

client.on('qr', (qr) => {
  qrData = qr;
  isReady = false;
  console.log('QR code received — scan to authenticate');
});

client.on('ready', () => {
  isReady = true;
  qrData = null;
  clientInfo = client.info;
  console.log('Client ready:', clientInfo?.pushname);
});

client.on('disconnected', (reason) => {
  isReady = false;
  console.log('Disconnected:', reason);
});

client.on('auth_failure', (msg) => {
  isReady = false;
  console.log('Auth failure:', msg);
});

// Store incoming messages
client.on('message', async (msg) => {
  try {
    const chat = await msg.getChat();
    const contact = await msg.getContact();
    messages.push({
      id: msg.id._serialized,
      from: msg.from,
      to: msg.to,
      author: msg.author || msg.from,
      body: msg.body,
      timestamp: msg.timestamp,
      type: msg.type,
      hasMedia: msg.hasMedia,
      isGroup: chat.isGroup,
      chatName: chat.name,
      contactName: contact.pushname || contact.name || contact.number || msg.from,
      isForwarded: msg.isForwarded,
      mentionedIds: msg._data.mentionedJidList || [],
      quotedMessageId: msg.hasQuotedMsg
        ? (msg._data.quotedMsg?.id?._serialized || msg._data.quotedStanzaID || null)
        : null
    });
    if (messages.length > MAX_MESSAGES) messages.shift();
    const last = messages[messages.length - 1];
    console.log(`[MSG] ${last.contactName} in ${last.chatName}: ${msg.body?.substring(0, 80)}`);
  } catch (e) {
    console.error('Error processing message:', e.message);
  }
});

// Store outgoing messages
client.on('message_create', async (msg) => {
  if (!msg.fromMe) return;
  try {
    const chat = await msg.getChat();
    messages.push({
      id: msg.id._serialized,
      from: msg.from,
      to: msg.to,
      author: 'me',
      body: msg.body,
      timestamp: msg.timestamp,
      type: msg.type,
      hasMedia: msg.hasMedia,
      isGroup: chat.isGroup,
      chatName: chat.name,
      contactName: 'Me',
      isForwarded: false,
      mentionedIds: [],
      quotedMessageId: msg.hasQuotedMsg
        ? (msg._data.quotedMsg?.id?._serialized || msg._data.quotedStanzaID || null)
        : null
    });
    if (messages.length > MAX_MESSAGES) messages.shift();
  } catch (e) {
    console.error('Error processing outgoing:', e.message);
  }
});

// ==================== HELPERS ====================

// Find a WWebJS message object by serialized ID
async function findMessage(messageId) {
  const msgEntry = messages.find(m => m.id === messageId);
  if (!msgEntry) return null;
  const chatId = msgEntry.isGroup
    ? msgEntry.from
    : (msgEntry.from === clientInfo?.wid?._serialized ? msgEntry.to : msgEntry.from);
  const chat = await client.getChatById(chatId);
  const chatMessages = await chat.fetchMessages({ limit: 100 });
  return chatMessages.find(m => m.id._serialized === messageId) || null;
}

// Resolve a chat ID from either a direct `to` number or a `group_name`
async function resolveChat(to, groupName) {
  if (groupName) {
    const chats = await client.getChats();
    const group = chats.find(c =>
      c.isGroup && c.name.toLowerCase().includes(groupName.toLowerCase())
    );
    if (!group) return null;
    return group.id._serialized;
  }
  return to.includes('@') ? to : `${to}@c.us`;
}

// Auth middleware
function auth(req, res, next) {
  const key = req.headers['x-api-key'] || req.query.key;
  if (key !== API_KEY) return res.status(401).json({ error: 'Unauthorized' });
  next();
}

// Connection guard
function requireReady(req, res, next) {
  if (!isReady) return res.status(503).json({ error: 'Not connected' });
  next();
}

// ==================== ENDPOINTS ====================

// ---------- Status ----------

// GET /health — Connection status & diagnostics
app.get('/health', auth, (req, res) => {
  res.json({
    status: isReady ? 'connected' : 'disconnected',
    hasQR: !!qrData,
    info: clientInfo
      ? { pushname: clientInfo.pushname, wid: clientInfo.wid?._serialized }
      : null,
    messageCount: messages.length,
    uptime: process.uptime()
  });
});

// GET /qr — Render QR code for WhatsApp Web authentication
app.get('/qr', auth, async (req, res) => {
  if (isReady) return res.json({ status: 'already_connected' });
  if (!qrData) return res.json({ status: 'waiting_for_qr' });
  try {
    const img = await qrcode.toDataURL(qrData);
    res.send(
      `<html><body style="display:flex;justify-content:center;align-items:center;` +
      `height:100vh;background:#111"><img src="${img}" style="width:400px;height:400px"/>` +
      `</body></html>`
    );
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Messages ----------

// GET /messages — Retrieve recent messages from the in-memory buffer
app.get('/messages', auth, (req, res) => {
  const limit = parseInt(req.query.limit) || 50;
  const since = parseInt(req.query.since) || 0;
  const chat = req.query.chat || null;
  let filtered = messages;
  if (since > 0) filtered = filtered.filter(m => m.timestamp > since);
  if (chat) filtered = filtered.filter(m => m.from === chat || m.to === chat);
  res.json({ count: filtered.length, messages: filtered.slice(-limit) });
});

// GET /chats — List recent chats
app.get('/chats', auth, requireReady, async (req, res) => {
  try {
    const chats = await client.getChats();
    const limit = parseInt(req.query.limit) || 30;
    const result = chats.slice(0, limit).map(c => ({
      id: c.id._serialized,
      name: c.name,
      isGroup: c.isGroup,
      unreadCount: c.unreadCount,
      lastMessage: c.lastMessage
        ? {
            body: c.lastMessage.body?.substring(0, 200),
            timestamp: c.lastMessage.timestamp,
            fromMe: c.lastMessage.fromMe
          }
        : null
    }));
    res.json({ count: result.length, chats: result });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /send — Send a text message to a phone number
app.post('/send', auth, requireReady, async (req, res) => {
  const { to, message, quotedMessageId } = req.body;
  if (!to || !message) return res.status(400).json({ error: 'Missing to or message' });
  try {
    const chatId = to.includes('@') ? to : `${to}@c.us`;
    const options = {};
    if (quotedMessageId) options.quotedMessageId = quotedMessageId;
    const result = await client.sendMessage(chatId, message, options);
    res.json({ success: true, id: result.id._serialized });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /send-group — Send a text message to a group by name
app.post('/send-group', auth, requireReady, async (req, res) => {
  const { group_name, message, quotedMessageId } = req.body;
  if (!group_name || !message) {
    return res.status(400).json({ error: 'Missing group_name or message' });
  }
  try {
    const chatId = await resolveChat(null, group_name);
    if (!chatId) return res.status(404).json({ error: 'Group not found' });
    const options = {};
    if (quotedMessageId) options.quotedMessageId = quotedMessageId;
    const result = await client.sendMessage(chatId, message, options);
    res.json({ success: true, id: result.id._serialized });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /send-media — Send an image, video, document, or audio file
app.post('/send-media', auth, requireReady, async (req, res) => {
  const { to, group_name, data, mimetype, filename, caption, quotedMessageId } = req.body;
  if ((!to && !group_name) || !data || !mimetype) {
    return res.status(400).json({ error: 'Missing to/group_name, data, or mimetype' });
  }
  try {
    const media = new MessageMedia(mimetype, data, filename || undefined);
    const options = { caption: caption || undefined };
    if (quotedMessageId) options.quotedMessageId = quotedMessageId;
    const chatId = await resolveChat(to, group_name);
    if (!chatId) return res.status(404).json({ error: 'Group not found' });
    const result = await client.sendMessage(chatId, media, options);
    res.json({ success: true, id: result.id._serialized });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Interactions ----------

// POST /react/:messageId — React to a message with an emoji
app.post('/react/:messageId', auth, requireReady, async (req, res) => {
  const { emoji } = req.body;
  if (emoji === undefined) return res.status(400).json({ error: 'Missing emoji' });
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    await wMsg.react(emoji);
    res.json({ success: true });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /forward/:messageId — Forward a message to another chat
app.post('/forward/:messageId', auth, requireReady, async (req, res) => {
  const { to, group_name } = req.body;
  if (!to && !group_name) {
    return res.status(400).json({ error: 'Missing to or group_name' });
  }
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    const chatId = await resolveChat(to, group_name);
    if (!chatId) return res.status(404).json({ error: 'Chat not found' });
    await wMsg.forward(chatId);
    res.json({ success: true });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /mark-read — Mark a chat as read
app.post('/mark-read', auth, requireReady, async (req, res) => {
  const { chatId } = req.body;
  if (!chatId) return res.status(400).json({ error: 'Missing chatId' });
  try {
    const cid = chatId.includes('@') ? chatId : `${chatId}@c.us`;
    const chat = await client.getChatById(cid);
    await chat.sendSeen();
    res.json({ success: true });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /pin/:messageId — Pin or unpin a message
app.post('/pin/:messageId', auth, requireReady, async (req, res) => {
  const { duration } = req.body; // seconds — 0 to unpin
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    if (duration === 0) {
      await wMsg.unpin();
      res.json({ success: true, action: 'unpinned' });
    } else {
      await wMsg.pin(duration || 604800); // default 7 days
      res.json({ success: true, action: 'pinned', duration: duration || 604800 });
    }
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// DELETE /message/:messageId — Delete a sent message
app.delete('/message/:messageId', auth, requireReady, async (req, res) => {
  const { everyone } = req.query;
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    await wMsg.delete(everyone === 'true');
    res.json({ success: true, deletedForEveryone: everyone === 'true' });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- History & Search ----------

// GET /history/:chatId — Fetch older messages from a chat
app.get('/history/:chatId', auth, requireReady, async (req, res) => {
  const limit = parseInt(req.query.limit) || 50;
  try {
    const cid = req.params.chatId.includes('@')
      ? req.params.chatId
      : `${req.params.chatId}@c.us`;
    const chat = await client.getChatById(cid);
    const msgs = await chat.fetchMessages({ limit });
    const result = msgs.map(m => ({
      id: m.id._serialized,
      from: m.from,
      body: m.body,
      timestamp: m.timestamp,
      type: m.type,
      hasMedia: m.hasMedia,
      fromMe: m.fromMe,
      quotedMessageId: m.hasQuotedMsg
        ? (m._data.quotedMsg?.id?._serialized || m._data.quotedStanzaID || null)
        : null
    }));
    res.json({ count: result.length, messages: result });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// GET /search — Search messages by keyword
app.get('/search', auth, requireReady, async (req, res) => {
  const { query, chatId, limit } = req.query;
  if (!query) return res.status(400).json({ error: 'Missing query' });
  try {
    const maxResults = parseInt(limit) || 20;
    let results;
    if (chatId) {
      const cid = chatId.includes('@') ? chatId : `${chatId}@c.us`;
      const chat = await client.getChatById(cid);
      results = await chat.searchMessages(query, { limit: maxResults });
    } else {
      results = await client.searchMessages(query, { limit: maxResults });
    }
    const mapped = results.map(m => ({
      id: m.id._serialized,
      from: m.from,
      body: m.body,
      timestamp: m.timestamp,
      type: m.type,
      fromMe: m.fromMe
    }));
    res.json({ count: mapped.length, messages: mapped });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Media ----------

// GET /media/:messageId — Download media from a message
app.get('/media/:messageId', auth, requireReady, async (req, res) => {
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    if (!wMsg.hasMedia) return res.status(400).json({ error: 'Message has no media' });
    const media = await wMsg.downloadMedia();
    if (!media) return res.status(404).json({ error: 'Media download failed' });
    res.json({
      mimetype: media.mimetype,
      filename: media.filename || null,
      data: media.data,
      size: media.data ? Buffer.from(media.data, 'base64').length : 0
    });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Contacts & Groups ----------

// GET /contact/:number — Get contact info
app.get('/contact/:number', auth, requireReady, async (req, res) => {
  try {
    const cid = req.params.number.includes('@')
      ? req.params.number
      : `${req.params.number}@c.us`;
    const contact = await client.getContactById(cid);
    const profilePic = await contact.getProfilePicUrl().catch(() => null);
    res.json({
      id: contact.id._serialized,
      name: contact.name,
      pushname: contact.pushname,
      number: contact.number,
      isBlocked: contact.isBlocked,
      isBusiness: contact.isBusiness,
      about: contact.about || null,
      profilePicUrl: profilePic || null
    });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// GET /group/:groupId — Get group info and participants
app.get('/group/:groupId', auth, requireReady, async (req, res) => {
  try {
    const gid = req.params.groupId.includes('@')
      ? req.params.groupId
      : `${req.params.groupId}@g.us`;
    const chat = await client.getChatById(gid);
    if (!chat.isGroup) return res.status(400).json({ error: 'Not a group' });
    const participants = chat.participants.map(p => ({
      id: p.id._serialized,
      isAdmin: p.isAdmin,
      isSuperAdmin: p.isSuperAdmin
    }));
    res.json({
      id: chat.id._serialized,
      name: chat.name,
      description: chat.description,
      createdAt: chat.createdAt,
      participantCount: participants.length,
      participants
    });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Polls ----------

// POST /poll — Create a poll in a group
app.post('/poll', auth, requireReady, async (req, res) => {
  const { group_name, chatId, title, options, allowMultiple } = req.body;
  if ((!group_name && !chatId) || !title || !options || !Array.isArray(options)) {
    return res.status(400).json({
      error: 'Missing group_name/chatId, title, or options array'
    });
  }
  try {
    let cid;
    if (chatId) {
      cid = chatId.includes('@') ? chatId : `${chatId}@g.us`;
    } else {
      cid = await resolveChat(null, group_name);
      if (!cid) return res.status(404).json({ error: 'Group not found' });
    }
    const poll = new Poll(title, options, {
      allowMultipleAnswers: !!allowMultiple
    });
    const result = await client.sendMessage(cid, poll);
    res.json({ success: true, id: result.id._serialized });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Session Management ----------

// POST /restart — Destroy and reinitialize the client
app.post('/restart', auth, async (req, res) => {
  try {
    await client.destroy();
    await client.initialize();
    res.json({ status: 'restarting' });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// POST /logout — Log out and clear session data
app.post('/logout', auth, async (req, res) => {
  try {
    await client.logout();
    res.json({ status: 'logged_out' });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ---------- Edit ----------

// PUT /edit/:messageId — Edit a sent message
app.put('/edit/:messageId', auth, requireReady, async (req, res) => {
  const { body: newBody } = req.body;
  if (!newBody) return res.status(400).json({ error: 'Missing body' });
  try {
    const wMsg = await findMessage(req.params.messageId);
    if (!wMsg) return res.status(404).json({ error: 'Message not found' });
    if (!wMsg.fromMe) return res.status(403).json({ error: 'Can only edit own messages' });
    await wMsg.edit(newBody);
    res.json({ success: true });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ==================== HTTP FORWARD PROXY ====================
// Routes HTTP requests through the NAS (residential IP) to bypass
// services that block datacenter/cloud IPs (Garmin, etc.)

const http = require('http');
const https = require('https');
const { URL } = require('url');

// In-memory cookie jar for proxy sessions
const cookieJars = new Map();

// POST /proxy — Forward an HTTP request through the NAS
app.post('/proxy', auth, async (req, res) => {
  const {
    url: targetUrl,
    method = 'GET',
    headers: reqHeaders = {},
    body: reqBody,
    cookies: reqCookies,
    timeout = 30000,
    followRedirects = true,
    maxRedirects = 10,
    encoding = 'utf8',      // 'utf8' or 'base64' for binary responses
    session: sessionId       // optional: persist cookies across requests
  } = req.body;

  if (!targetUrl) {
    return res.status(400).json({ error: 'Missing url' });
  }

  try {
    const result = await proxyRequest({
      url: targetUrl,
      method: method.toUpperCase(),
      headers: reqHeaders,
      body: reqBody,
      cookies: reqCookies,
      timeout,
      followRedirects,
      maxRedirects,
      encoding,
      sessionId
    });
    res.json(result);
  } catch (e) {
    res.status(502).json({ error: e.message });
  }
});

// POST /proxy/session — Manage cookie sessions
app.post('/proxy/session', auth, (req, res) => {
  const { action, session } = req.body;
  if (!session) return res.status(400).json({ error: 'Missing session id' });

  switch (action) {
    case 'create':
      cookieJars.set(session, []);
      return res.json({ success: true, session });
    case 'cookies':
      return res.json({ cookies: cookieJars.get(session) || [] });
    case 'destroy':
      cookieJars.delete(session);
      return res.json({ success: true });
    default:
      return res.status(400).json({ error: 'Unknown action. Use: create, cookies, destroy' });
  }
});

async function proxyRequest(opts) {
  let {
    url: targetUrl, method, headers, body, cookies,
    timeout, followRedirects, maxRedirects, encoding, sessionId
  } = opts;

  let redirectCount = 0;
  let allSetCookies = [];

  // Load session cookies if session exists
  if (sessionId && cookieJars.has(sessionId)) {
    const jarCookies = cookieJars.get(sessionId);
    const existing = cookies || '';
    cookies = [...jarCookies.map(c => c.split(';')[0]), ...(existing ? [existing] : [])].join('; ');
  }

  while (true) {
    const parsed = new URL(targetUrl);
    const isHttps = parsed.protocol === 'https:';
    const transport = isHttps ? https : http;

    const requestHeaders = { ...headers };
    if (cookies) requestHeaders['cookie'] = cookies;
    if (!requestHeaders['user-agent']) {
      requestHeaders['user-agent'] = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36';
    }

    // Set content-length for body
    let bodyBuffer = null;
    if (body && method !== 'GET' && method !== 'HEAD') {
      bodyBuffer = typeof body === 'string' ? Buffer.from(body) : Buffer.from(body, 'base64');
      requestHeaders['content-length'] = bodyBuffer.length;
    }

    const response = await new Promise((resolve, reject) => {
      const reqOpts = {
        hostname: parsed.hostname,
        port: parsed.port || (isHttps ? 443 : 80),
        path: parsed.pathname + parsed.search,
        method,
        headers: requestHeaders,
        timeout,
        rejectUnauthorized: true
      };

      const request = transport.request(reqOpts, (res) => {
        const chunks = [];
        res.on('data', chunk => chunks.push(chunk));
        res.on('end', () => {
          const buffer = Buffer.concat(chunks);
          resolve({
            status: res.statusCode,
            headers: res.headers,
            rawHeaders: res.rawHeaders,
            buffer
          });
        });
      });

      request.on('error', reject);
      request.on('timeout', () => {
        request.destroy();
        reject(new Error(`Timeout after ${timeout}ms`));
      });

      if (bodyBuffer) request.write(bodyBuffer);
      request.end();
    });

    // Collect set-cookie headers
    const setCookies = response.headers['set-cookie'] || [];
    allSetCookies.push(...setCookies);

    // Update session cookie jar
    if (sessionId) {
      if (!cookieJars.has(sessionId)) cookieJars.set(sessionId, []);
      const jar = cookieJars.get(sessionId);
      for (const sc of setCookies) {
        const name = sc.split('=')[0].trim();
        const idx = jar.findIndex(c => c.split('=')[0].trim() === name);
        if (idx >= 0) jar[idx] = sc;
        else jar.push(sc);
      }
      // Update cookies for next redirect
      cookies = jar.map(c => c.split(';')[0]).join('; ');
    }

    // Handle redirects
    if (followRedirects && [301, 302, 303, 307, 308].includes(response.status)) {
      if (++redirectCount > maxRedirects) {
        throw new Error(`Too many redirects (max ${maxRedirects})`);
      }
      const location = response.headers['location'];
      if (!location) throw new Error('Redirect without Location header');
      // Resolve relative URLs
      targetUrl = new URL(location, targetUrl).toString();
      // 303 always converts to GET
      if (response.status === 303) {
        method = 'GET';
        body = null;
        bodyBuffer = null;
      }
      continue;
    }

    // Build response
    const responseBody = encoding === 'base64'
      ? response.buffer.toString('base64')
      : response.buffer.toString('utf8');

    // Clean up response headers (remove hop-by-hop)
    const cleanHeaders = {};
    for (const [k, v] of Object.entries(response.headers)) {
      if (!['transfer-encoding', 'connection', 'keep-alive'].includes(k)) {
        cleanHeaders[k] = v;
      }
    }

    return {
      status: response.status,
      headers: cleanHeaders,
      cookies: allSetCookies,
      body: responseBody,
      url: targetUrl,
      redirectCount
    };
  }
}

// ==================== START ====================

client.initialize();
app.listen(PORT, () => console.log(`wwebjs-wa listening on port ${PORT}`));

import schema from './schema.sql';

const encoder = new TextEncoder();
const IMAGE_ID = /^[A-Za-z0-9_-]{32}$/;
const SETTINGS = {
  max_active: [1, 100000],
  max_image_bytes: [1024, 104857600],
  max_storage_bytes: [1048576, 1000000000000],
  daily_upload_limit: [1, 100000],
  daily_view_limit: [1, 100000000],
  retention_days: [1, 3650],
  warning_percent: [1, 100],
};
const NO_STORE = {
  'Cache-Control': 'no-store, max-age=0',
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
};

function json(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { ...NO_STORE, 'Content-Type': 'application/json; charset=utf-8' },
  });
}
function error(status, message) {
  return json({ error: message }, status);
}
function dayOf(now) { return new Date(now).toISOString().slice(0, 10); }
function monthOf(now) { return dayOf(now).slice(0, 7); }
function randomId(bytes = 24) {
  return btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(bytes))))
    .replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
}
function decodeBase64Url(value) {
  const base = value.replaceAll('-', '+').replaceAll('_', '/');
  return Uint8Array.from(atob(base.padEnd(Math.ceil(base.length / 4) * 4, '=')), ch => ch.charCodeAt(0));
}
function encodeBase64Url(bytes) {
  return btoa(String.fromCharCode(...bytes)).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
}
function constantEqual(left, right) {
  if (left.length !== right.length) return false;
  let different = 0;
  for (let i = 0; i < left.length; i++) different |= left[i] ^ right[i];
  return different === 0;
}
async function authorized(request, secret) {
  const header = request.headers.get('Authorization') || '';
  if (!secret || !/^Bearer [A-Za-z0-9_-]{40,128}$/.test(header)) return false;
  const presented = encoder.encode(header.slice(7));
  const expected = encoder.encode(secret);
  const [left, right] = await Promise.all([
    crypto.subtle.digest('SHA-256', presented),
    crypto.subtle.digest('SHA-256', expected),
  ]);
  return constantEqual(new Uint8Array(left), new Uint8Array(right));
}
function originFor(request, env) {
  const url = new URL(request.url);
  if (url.protocol !== 'https:' && url.hostname !== 'localhost' && url.hostname !== '127.0.0.1')
    throw new Error('This endpoint requires HTTPS.');
  if (env.PUBLIC_ORIGIN && new URL(env.PUBLIC_ORIGIN).origin !== url.origin)
    throw new Error('The request hostname does not match this installation.');
  return url.origin;
}
async function limitedBody(request, maximum) {
  if (!request.body) throw new RangeError('Missing request body.');
  const reader = request.body.getReader();
  const chunks = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > maximum) throw new RangeError('Request body is too large.');
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.byteLength; }
  return result;
}
async function ready(db) {
  const complete = await db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='password_attempts'").first();
  if (!complete) {
    for (const statement of schema.split('-- @@').map(part => part.trim()).filter(Boolean)) {
      await db.prepare(statement).run();
    }
  }
  const settings = await db.prepare('SELECT * FROM preferences WHERE id = 1').first();
  if (!settings) throw new Error('The installation settings are missing.');
  return settings;
}
async function digestPassword(password, salt) {
  const key = await crypto.subtle.importKey('raw', encoder.encode(password), 'PBKDF2', false, ['deriveBits']);
  const result = await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations: 30000 }, key, 256);
  return new Uint8Array(result);
}
function safePasswordHeader(request) {
  const raw = request.headers.get('X-Image-Password');
  if (raw === null) return null;
  if (!/^[A-Za-z0-9_-]{16,172}$/.test(raw)) throw new RangeError('Use a longer image password.');
  let bytes;
  try { bytes = decodeBase64Url(raw); } catch { throw new RangeError('Invalid password encoding.'); }
  let password;
  try { password = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { throw new RangeError('Invalid password encoding.'); }
  if (password.length < 12 || password.length > 128 || /[\x00-\x1f\x7f]/.test(password))
    throw new RangeError('Image password must contain 12–128 printable characters.');
  return password;
}
function contentType(request) {
  const type = request.headers.get('Content-Type');
  if (type !== 'image/png' && type !== 'image/jpeg') throw new RangeError('Only PNG and JPEG are accepted.');
  return type;
}
async function imageBody(request, maximum, type) {
  if (!request.body) throw new RangeError('No screenshot was sent.');
  const lengthHeader = request.headers.get('Content-Length');
  if (lengthHeader && (!/^\d+$/.test(lengthHeader) || Number(lengthHeader) > maximum))
    throw new RangeError('Image exceeds the configured size limit.');
  const reader = request.body.getReader();
  const firstChunks = [];
  const prefix = new Uint8Array(24);
  let prefixLength = 0;
  let size = 0;
  while (prefixLength < 24) {
    const next = await reader.read();
    if (next.done) break;
    size += next.value.byteLength;
    if (size > maximum) { await reader.cancel(); throw new RangeError('Image exceeds the configured size limit.'); }
    firstChunks.push(next.value);
    const part = Math.min(24 - prefixLength, next.value.byteLength);
    prefix.set(next.value.subarray(0, part), prefixLength);
    prefixLength += part;
  }
  if (prefixLength < 24) { await reader.cancel(); throw new RangeError('Incomplete image.'); }
  if (type === 'image/png') {
    if (!constantEqual(prefix.subarray(0, 8), Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10)) ||
      prefix[12] !== 73 || prefix[13] !== 72 || prefix[14] !== 68 || prefix[15] !== 82)
      { await reader.cancel(); throw new RangeError('Invalid PNG image.'); }
    const dv = new DataView(prefix.buffer);
    const w = dv.getUint32(16), h = dv.getUint32(20);
    if (!w || !h || w > 16384 || h > 16384 || w * h > 100000000)
      { await reader.cancel(); throw new RangeError('Image dimensions exceed the limit.'); }
  } else if (prefix[0] !== 255 || prefix[1] !== 216 || prefix[2] !== 255) {
    await reader.cancel(); throw new RangeError('Invalid JPEG image.');
  }
  let index = 0;
  const stream = new ReadableStream({
    async pull(controller) {
      if (index < firstChunks.length) { controller.enqueue(firstChunks[index++]); return; }
      try {
        const next = await reader.read();
        if (next.done) { controller.close(); return; }
        size += next.value.byteLength;
        if (size > maximum) throw new RangeError('Image exceeds the configured size limit.');
        controller.enqueue(next.value);
      } catch (failure) { controller.error(failure); await reader.cancel(failure); }
    },
    async cancel(reason) { await reader.cancel(reason); },
  });
  return { stream, byteCount: () => size };
}
async function markDeleting(db, id, now) {
  await db.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE id = ? AND state != 'deleting'").bind(now, id).run();
}
async function purge(db, bucket, count = 25) {
  const { results } = await db.prepare("SELECT id, object_key FROM images WHERE state = 'deleting' ORDER BY deleted_at LIMIT ?").bind(count).all();
  for (const item of results) {
    await bucket.delete(item.object_key);
    await db.prepare("DELETE FROM images WHERE id = ? AND state = 'deleting'").bind(item.id).run();
  }
}
async function trim(db, now) {
  await db.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE state = 'active' AND (expires_at <= ? OR id IN (SELECT id FROM images WHERE state = 'active' ORDER BY created_at DESC, rowid DESC LIMIT -1 OFFSET (SELECT max_active FROM preferences WHERE id = 1)))").bind(now, now).run();
}
function quotaError(failure) {
  const text = String(failure);
  return /quota_uploads|quota_views|quota_storage/.test(text);
}
function isMissingTable(failure) { return /no such table/i.test(String(failure)); }
async function limitReached(db, settings, now, reading) {
  if (settings.limit_action === 'warn' || (reading && settings.limit_action !== 'block_all')) return false;
  const today = await db.prepare('SELECT uploads, views FROM usage_daily WHERE day = ?').bind(dayOf(now)).first();
  const uploads = today?.uploads || 0, views = today?.views || 0;
  const uploadCap = Math.max(1, Math.floor(settings.daily_upload_limit * settings.warning_percent / 100));
  const viewCap = Math.max(1, Math.floor(settings.daily_view_limit * settings.warning_percent / 100));
  const occupied = await db.prepare("SELECT COALESCE(SUM(size_bytes), 0) bytes FROM images WHERE state = 'active'").first();
  const storageFull = occupied.bytes >= settings.max_storage_bytes * settings.warning_percent / 100;
  return uploads >= uploadCap || storageFull || (settings.limit_action === 'block_all' && views >= viewCap);
}
async function upload(request, env, settings, now, origin, ctx) {
  if (await limitReached(env.DB, settings, now, false)) return error(429, 'Service limit reached. Change limits or wait for reset.');
  const type = contentType(request);
  const optionalPassword = safePasswordHeader(request);
  const maxSize = settings.max_image_bytes;
  const body = await imageBody(request, maxSize, type);
  const id = randomId();
  const key = `images/${id}`;
  const stamp = Math.floor(now / 1000);
  const salt = optionalPassword ? crypto.getRandomValues(new Uint8Array(16)) : null;
  const hash = salt ? await digestPassword(optionalPassword, salt) : null;
  await env.DB.prepare("INSERT INTO images(id, object_key, state, content_type, size_bytes, created_at, expires_at, password_salt, password_hash) VALUES (?, ?, 'pending', ?, 0, ?, ?, ?, ?)")
    .bind(id, key, type, stamp, stamp + settings.retention_days * 86400, salt && encodeBase64Url(salt), hash && encodeBase64Url(hash)).run();
  try {
    await env.IMAGES.put(key, body.stream, { httpMetadata: { contentType: type } });
    const bytes = body.byteCount();
    if (bytes < 24) throw new RangeError('Incomplete image.');
    const today = dayOf(now);
    await env.DB.batch([
      env.DB.prepare("INSERT INTO usage_daily(day, uploads, views, bytes_uploaded) VALUES (?, 1, 0, ?) ON CONFLICT(day) DO UPDATE SET uploads = uploads + 1, bytes_uploaded = bytes_uploaded + excluded.bytes_uploaded").bind(today, bytes),
      env.DB.prepare("UPDATE images SET state = 'active', size_bytes = ? WHERE id = ? AND state = 'pending'").bind(bytes, id),
      env.DB.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE state = 'active' AND id IN (SELECT id FROM images WHERE state = 'active' ORDER BY created_at DESC, rowid DESC LIMIT -1 OFFSET (SELECT max_active FROM preferences WHERE id = 1))").bind(stamp),
    ]);
    const row = await env.DB.prepare('SELECT state FROM images WHERE id = ?').bind(id).first();
    if (row?.state !== 'active') throw new Error('The upload was cancelled during cleanup.');
    ctx.waitUntil(purge(env.DB, env.IMAGES).catch(failure => console.error('Deferred deletion failed', String(failure))));
    return json({ id, url: `${origin}/i/${id}` }, 201);
  } catch (failure) {
    await markDeleting(env.DB, id, stamp);
    try { await env.IMAGES.delete(key); await env.DB.prepare('DELETE FROM images WHERE id = ?').bind(id).run(); }
    catch (cleanupFailure) { console.error('Orphan cleanup failed', String(cleanupFailure)); }
    if (failure instanceof RangeError) throw failure;
    if (quotaError(failure)) return error(429, 'Upload quota reached. Change limits in Cloud settings or wait for reset.');
    throw failure;
  }
}
async function countView(db, id, now) {
  await db.batch([
    db.prepare('INSERT INTO usage_daily(day, uploads, views, bytes_uploaded) VALUES (?, 0, 1, 0) ON CONFLICT(day) DO UPDATE SET views = views + 1').bind(dayOf(now)),
    db.prepare("UPDATE images SET views = views + 1 WHERE id = ? AND state = 'active'").bind(id),
  ]);
}
async function sendImage(env, record, now) {
  const object = await env.IMAGES.get(record.object_key);
  if (!object) { await markDeleting(env.DB, record.id, Math.floor(now / 1000)); return error(404, 'Image not found.'); }
  try { await countView(env.DB, record.id, now); }
  catch (failure) {
    if (quotaError(failure)) { await object.body.cancel(); return error(429, 'View quota reached.'); }
    await object.body.cancel(); throw failure;
  }
  return new Response(object.body, {
    headers: {
      ...NO_STORE,
      'Content-Type': record.content_type,
      'Content-Security-Policy': "default-src 'none'; sandbox",
      'Content-Disposition': `inline; filename="isolmass.${record.content_type === 'image/png' ? 'png' : 'jpg'}"`,
    },
  });
}
function passwordPage(id) {
  const html = `<!doctype html><html lang="tr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Şifreli görüntü · isolmaSS</title><style>body{font:16px system-ui;margin:8vh auto;padding:1rem;max-width:34rem;background:#101821;color:#f4f6f8}form{display:grid;gap:1rem}input,button{font:inherit;padding:.8rem}button{cursor:pointer}</style><main><h1>Bu görüntü şifreli</h1><p>Şifreyi paylaşan kişiden isteyin.</p><form method="post" action="/i/${id}/unlock"><label for="password">Görüntü şifresi</label><input id="password" name="password" type="password" required autocomplete="off"><button>Görüntüyü aç</button></form></main></html>`;
  return new Response(html, {
    headers: { ...NO_STORE, 'Content-Type': 'text/html; charset=utf-8', 'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'", 'X-Frame-Options': 'DENY' },
  });
}
async function view(request, env, id, now, unlock = false) {
  if (!IMAGE_ID.test(id)) return error(404, 'Image not found.');
  let record;
  try { record = await env.DB.prepare("SELECT * FROM images WHERE id = ? AND state = 'active' AND expires_at > ?").bind(id, Math.floor(now / 1000)).first(); }
  catch (failure) { if (isMissingTable(failure)) return error(404, 'Image not found.'); throw failure; }
  if (!record) return error(404, 'Image not found.');
  if (await limitReached(env.DB, await readSettings(env.DB), now, true)) return error(429, 'Service limit reached.');
  if (!record.password_hash) return unlock ? error(405, 'This image is not password protected.') : sendImage(env, record, now);
  if (!unlock) return passwordPage(id);
  const hour = Math.floor(now / 3600000);
  const attempts = await env.DB.prepare('SELECT failures FROM password_attempts WHERE id = ? AND hour = ?').bind(id, hour).first();
  if (attempts?.failures >= 20) return error(429, 'Too many attempts. Try again in one hour.');
  const body = await limitedBody(request, 4096);
  const password = new URLSearchParams(new TextDecoder().decode(body)).get('password') || '';
  if (password.length > 128) return error(400, 'Invalid password.');
  const hash = await digestPassword(password, decodeBase64Url(record.password_salt));
  if (!constantEqual(hash, decodeBase64Url(record.password_hash))) {
    await env.DB.prepare('INSERT INTO password_attempts(id, hour, failures) VALUES (?, ?, 1) ON CONFLICT(id, hour) DO UPDATE SET failures = failures + 1').bind(id, hour).run();
    return error(401, 'Wrong password.');
  }
  return sendImage(env, record, now);
}
async function readSettings(db) { return db.prepare('SELECT * FROM preferences WHERE id = 1').first(); }
async function updateSettings(request, env, now) {
  const body = await limitedBody(request, 4096);
  let value;
  try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body)); }
  catch { return error(400, 'Invalid settings JSON.'); }
  if (!value || Array.isArray(value) || typeof value !== 'object') return error(400, 'Settings must be an object.');
  const pairs = Object.entries(value);
  if (pairs.length === 0) return error(400, 'No settings provided.');
  for (const [field, input] of pairs) {
    if (field === 'limit_action') {
      if (!['warn', 'block_upload', 'block_all'].includes(input)) return error(400, 'Invalid limit action.');
    } else if (!SETTINGS[field] || !Number.isSafeInteger(input) || input < SETTINGS[field][0] || input > SETTINGS[field][1]) {
      return error(400, `Invalid ${field}.`);
    }
  }
  const sql = pairs.map(([key]) => `${key} = ?`).join(', ');
  const updated = await env.DB.prepare(`UPDATE preferences SET ${sql} WHERE id = 1`).bind(...pairs.map(([, data]) => data)).run();
  if (!updated.meta.changes) throw new Error('Settings update failed.');
  const settings = await readSettings(env.DB);
  await env.DB.batch([
    env.DB.prepare("UPDATE images SET expires_at = created_at + ? WHERE state = 'active'").bind(settings.retention_days * 86400),
    env.DB.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE state = 'active' AND (expires_at <= ? OR id IN (SELECT id FROM images WHERE state = 'active' ORDER BY created_at DESC, rowid DESC LIMIT -1 OFFSET ?))").bind(Math.floor(now / 1000), Math.floor(now / 1000), settings.max_active),
  ]);
  return json(settings);
}
async function stats(db, now) {
  const today = dayOf(now), month = monthOf(now);
  const [settings, current, monthly, totals, images] = await Promise.all([
    readSettings(db),
    db.prepare('SELECT uploads, views, bytes_uploaded FROM usage_daily WHERE day = ?').bind(today).first(),
    db.prepare('SELECT COALESCE(SUM(uploads),0) uploads, COALESCE(SUM(views),0) views, COALESCE(SUM(bytes_uploaded),0) bytes_uploaded FROM usage_daily WHERE day >= ? AND day < ?').bind(`${month}-01`, `${month}-32`).first(),
    db.prepare('SELECT COALESCE(SUM(uploads),0) uploads, COALESCE(SUM(views),0) views FROM usage_daily').first(),
    db.prepare("SELECT COUNT(*) active, COALESCE(SUM(size_bytes),0) stored_bytes FROM images WHERE state = 'active' AND expires_at > ?").bind(Math.floor(now / 1000)).first(),
  ]);
  const daily = current || { uploads: 0, views: 0, bytes_uploaded: 0 };
  const workerRequests = monthly.uploads + monthly.views * 2;
  const estimated = {
    worker_requests: workerRequests,
    r2_reads: monthly.views,
    paid_usd: Number((5 + Math.max(0, workerRequests - 10000000) * 0.30 / 1000000
      + Math.max(0, workerRequests * 2 - 30000000) * 0.02 / 1000000
      + Math.max(0, monthly.views - 10000000) * 0.36 / 1000000
      + Math.max(0, monthly.uploads - 1000000) * 4.50 / 1000000
      + Math.max(0, images.stored_bytes / 1000000000 - 10) * 0.015).toFixed(2)),
    note: 'Estimate assumes 2 ms CPU/request and current stored bytes for a whole month. Other account traffic and Cloudflare billing are excluded.',
  };
  return { daily, monthly, totals, images, settings, estimates: estimated, warning: {
    uploads: daily.uploads >= settings.daily_upload_limit * settings.warning_percent / 100,
    views: daily.views >= settings.daily_view_limit * settings.warning_percent / 100,
    storage: images.stored_bytes >= settings.max_storage_bytes * settings.warning_percent / 100,
  } };
}
async function listImages(db, request, origin) {
  const url = new URL(request.url);
  const limit = Number(url.searchParams.get('limit') || 50);
  const offset = Number(url.searchParams.get('offset') || 0);
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100 || !Number.isSafeInteger(offset) || offset < 0 || offset > 100000)
    return error(400, 'Invalid page.');
  const { results } = await db.prepare("SELECT id, created_at, expires_at, size_bytes, views, password_hash IS NOT NULL AS password_protected FROM images WHERE state = 'active' ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?").bind(limit, offset).all();
  return json({ images: results.map(row => ({ ...row, url: `${origin}/i/${row.id}` })), next_offset: results.length === limit ? offset + limit : null });
}
async function admin(request, env, path, origin, now, ctx) {
  if (!await authorized(request, env.ADMIN_TOKEN)) return error(401, 'Administrator token required.');
  await ready(env.DB);
  if (path === '/api/setup' && request.method === 'POST') return json({ status: 'ready', origin });
  if (path === '/api/settings') {
    if (request.method === 'GET') return json(await readSettings(env.DB));
    if (request.method === 'PUT') return updateSettings(request, env, now);
  }
  if (path === '/api/stats' && request.method === 'GET') return json(await stats(env.DB, now));
  if (path === '/api/images' && request.method === 'GET') return listImages(env.DB, request, origin);
  const deletion = /^\/api\/images\/([A-Za-z0-9_-]{32})$/.exec(path);
  if (deletion && request.method === 'DELETE') {
    const result = await env.DB.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE id = ? AND state = 'active'").bind(Math.floor(now / 1000), deletion[1]).run();
    if (!result.meta.changes) return error(404, 'Image not found.');
    ctx.waitUntil(purge(env.DB, env.IMAGES).catch(failure => console.error('Deferred deletion failed', String(failure))));
    return json({ deleted: true });
  }
  return error(405, 'Unsupported operation.');
}
export default {
  async fetch(request, env, ctx) {
    const now = Date.now();
    const path = new URL(request.url).pathname;
    let origin;
    try { origin = originFor(request, env); }
    catch { return error(421, 'Unknown or insecure hostname.'); }
    try {
      if (path.startsWith('/api/settings') || path === '/api/stats' || path === '/api/images' || path === '/api/setup' || path.startsWith('/api/images/'))
        return admin(request, env, path, origin, now, ctx);
      if (path === '/api/upload') {
        if (request.method !== 'POST') return error(405, 'POST required.');
        if (!await authorized(request, env.UPLOAD_TOKEN)) return error(401, 'Upload token required.');
        const settings = await ready(env.DB);
        return await upload(request, env, settings, now, origin, ctx);
      }
      const display = /^\/i\/([^/]+)$/.exec(path);
      if (display && request.method === 'GET') return view(request, env, display[1], now);
      const unlock = /^\/i\/([^/]+)\/unlock$/.exec(path);
      if (unlock && request.method === 'POST') return view(request, env, unlock[1], now, true);
      return error(404, 'Not found.');
    } catch (failure) {
      if (failure instanceof RangeError || failure instanceof SyntaxError) return error(400, failure.message);
      console.error('Share request failed', String(failure));
      return error(503, 'Service temporarily unavailable.');
    }
  },
  async scheduled(_controller, env, ctx) {
    ctx.waitUntil((async () => {
      await ready(env.DB);
      const now = Math.floor(Date.now() / 1000);
      await env.DB.prepare("UPDATE images SET state = 'deleting', deleted_at = ? WHERE state = 'pending' AND created_at < ?").bind(now, now - 3600).run();
      await trim(env.DB, now);
      await purge(env.DB, env.IMAGES);
      await env.DB.prepare('DELETE FROM password_attempts WHERE hour < ?').bind(Math.floor(now / 3600) - 24).run();
    })());
  },
};

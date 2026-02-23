# Media Integration Guide

Connect your own media player to Fortemi's streaming endpoints. This guide covers
video/audio playback, subtitle tracks, thumbnail sprite sheets, resumable uploads, and
real-time processing events. Code examples use React, drawn from the
[HotM](https://github.com/fortemi/hotm) reference frontend.

---

## Endpoint Reference

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v1/attachments/{id}/download` | GET | Stream or download the original file |
| `/api/v1/attachments/{id}/download?variant={v}` | GET | Download a pre-optimized media variant |
| `/api/v1/attachments/{id}/thumbnail` | GET | Poster / preview image (JPEG) |
| `/api/v1/attachments/{id}/subtitles?format=vtt` | GET | WebVTT captions |
| `/api/v1/attachments/{id}/subtitles?format=srt` | GET | SRT captions |
| `/api/v1/attachments/{id}/thumbnails.vtt` | GET | Sprite-sheet VTT for seek-bar previews |
| `/api/v1/attachments/{id}/sprites/{index}` | GET | Sprite-sheet JPEG (5x5 grid, 160x90 each) |
| `/api/v1/attachments/{id}` | GET | Full attachment metadata (JSON) |
| `/api/v1/notes/{id}/attachments/tus` | POST | Create a TUS resumable upload session |
| `/api/v1/events` | GET | SSE stream for real-time processing events |

All endpoints respect the `X-Fortemi-Memory` header for multi-memory routing (see
[Multi-Memory Architecture](multi-memory.md)).

---

## 1. Streaming Playback

### How It Works

Fortemi serves media files with full HTTP Range request support (RFC 7233). The browser
handles partial content (206 responses) transparently when you point a `<video>` or
`<audio>` element at the download URL.

**Response headers on the download endpoint:**

| Header | Value |
|--------|-------|
| `Content-Type` | Actual MIME type of the file |
| `Content-Disposition` | `inline` for video/audio/image/pdf; `attachment` for others |
| `Accept-Ranges` | `bytes` |
| `Content-Range` | `bytes start-end/total` (on 206 responses) |
| `Content-Length` | Size of the returned range or full file |

### Basic Video Player

The simplest integration points an HTML5 `<video>` element directly at the download URL.
The browser handles Range requests, buffering, and progressive playback automatically.

```tsx
function VideoPlayer({ attachmentId }: { attachmentId: string }) {
  const src = `/api/v1/attachments/${attachmentId}/download`;
  const poster = `/api/v1/attachments/${attachmentId}/thumbnail`;

  return (
    <video
      src={src}
      poster={poster}
      controls
      playsInline
      preload="metadata"
    />
  );
}
```

### Basic Audio Player

```tsx
function AudioPlayer({ attachmentId }: { attachmentId: string }) {
  const src = `/api/v1/attachments/${attachmentId}/download`;

  return <audio src={src} controls preload="metadata" />;
}
```

### Choosing a Media Variant

Fortemi pre-generates optimized variants during the media optimization pipeline. Query the
attachment metadata to discover which variants are available, then pass the variant name as
a query parameter.

**Available video variants:**

| Variant | Description |
|---------|-------------|
| `faststart` | MP4 with moov atom relocated for progressive download (copy-only, no re-encode) |
| `web_compatible` | MKV/MOV remuxed to H.264+AAC MP4 for maximum browser support |
| `preview_720p` | Downscaled 720p H.264 preview (transcoded, only for large files) |

**Available audio variants:**

| Variant | Description |
|---------|-------------|
| `web_audio` | Transcoded to AAC/MP3 for universal browser playback |
| `audio_only` | Audio track extracted from a video file |
| `audio_preview` | Lower-bitrate audio preview |

**Selecting a variant:**

```tsx
// Fetch attachment metadata to discover variants
const res = await fetch(`/api/v1/attachments/${id}`);
const attachment = await res.json();

const available = attachment.extracted_metadata?.available_variants ?? [];

// Preference order for video
const VIDEO_PREFS = ['web_compatible', 'faststart', 'preview_720p'];
const variant = VIDEO_PREFS.find(v => available.includes(v));

// Build the URL
const src = variant
  ? `/api/v1/attachments/${id}/download?variant=${variant}`
  : `/api/v1/attachments/${id}/download`;
```

**Preference order used in HotM:**

- Video: `web_compatible` > `faststart` > `preview_720p` > original
- Audio: `web_audio` > `audio_only` > `audio_preview` > original

---

## 2. Subtitles and Captions

Fortemi generates captions from audio transcription (Whisper) and optionally adds speaker
labels from diarization (pyannote). Captions are available in three formats.

### Server-Generated Subtitle Tracks

```tsx
function VideoWithCaptions({ attachmentId }: { attachmentId: string }) {
  return (
    <video
      src={`/api/v1/attachments/${attachmentId}/download`}
      controls
    >
      <track
        kind="subtitles"
        src={`/api/v1/attachments/${attachmentId}/subtitles?format=vtt`}
        srcLang="en"
        label="English"
        default
      />
    </video>
  );
}
```

The `?format=` parameter accepts `vtt`, `srt`, or `rttm`.

### Client-Side Captions from Metadata

Transcript segments are also embedded in the attachment's `extracted_metadata`. This avoids
an extra network request and enables interactive transcript panels.

```tsx
interface TranscriptSegment {
  start_secs: number;
  end_secs: number;
  text: string;
  speaker?: string;         // Speaker name (from diarization)
  speaker_id?: string;      // Raw speaker ID
  words?: Array<{           // Word-level timestamps
    word: string;
    start: number;
    end: number;
    confidence?: number;
  }>;
}
```

**Converting segments to a WebVTT blob URL:**

```ts
function segmentsToVtt(segments: TranscriptSegment[]): string {
  let vtt = 'WEBVTT\n\n';
  for (const seg of segments) {
    const start = formatVttTime(seg.start_secs);
    const end = formatVttTime(seg.end_secs);
    const speaker = seg.speaker ? `<v ${seg.speaker}>` : '';
    vtt += `${start} --> ${end}\n${speaker}${seg.text}\n\n`;
  }
  return vtt;
}

function formatVttTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  const ms = Math.round((secs % 1) * 1000);
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}.${String(ms).padStart(3, '0')}`;
}

// Create a blob URL for <track src="...">
function createVttBlobUrl(segments: TranscriptSegment[]): string {
  const blob = new Blob([segmentsToVtt(segments)], { type: 'text/vtt' });
  return URL.createObjectURL(blob);
}
```

### Interactive Transcript Panel

HotM implements a clickable transcript panel that syncs with the video playhead:

```tsx
function TranscriptPanel({
  segments,
  currentTime,
  onSeek,
}: {
  segments: TranscriptSegment[];
  currentTime: number;
  onSeek: (secs: number) => void;
}) {
  const activeRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to active segment
  useEffect(() => {
    activeRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }, [currentTime]);

  return (
    <div style={{ maxHeight: 300, overflow: 'auto' }}>
      {segments.map((seg, i) => {
        const active = currentTime >= seg.start_secs && currentTime < seg.end_secs;
        return (
          <div
            key={i}
            ref={active ? activeRef : undefined}
            onClick={() => onSeek(seg.start_secs)}
            style={{
              padding: '4px 8px',
              cursor: 'pointer',
              background: active ? '#e0e7ff' : 'transparent',
            }}
          >
            <span style={{ fontSize: 11, color: '#666' }}>
              {formatTimestamp(seg.start_secs)}
            </span>
            {seg.speaker && (
              <span style={{ fontSize: 11, fontWeight: 600 }}>
                {' '}{seg.speaker}:
              </span>
            )}
            <span> {seg.text}</span>
          </div>
        );
      })}
    </div>
  );
}
```

Wire it to a `<video>` element:

```tsx
const videoRef = useRef<HTMLVideoElement>(null);
const [currentTime, setCurrentTime] = useState(0);

<video
  ref={videoRef}
  onTimeUpdate={() => setCurrentTime(videoRef.current?.currentTime ?? 0)}
  ...
/>
<TranscriptPanel
  segments={segments}
  currentTime={currentTime}
  onSeek={(t) => { if (videoRef.current) videoRef.current.currentTime = t; }}
/>
```

---

## 3. Thumbnail Sprite Sheets

For video files, Fortemi generates CSS sprite sheets from keyframes with a WebVTT map that
ties each sprite region to a timestamp range. This enables seek-bar thumbnail previews
(hover over the progress bar to see a frame preview).

### How It Works

1. **VTT map** at `/api/v1/attachments/{id}/thumbnails.vtt` contains time ranges pointing
   to sprite regions:

   ```vtt
   WEBVTT

   00:00:00.000 --> 00:00:05.000
   /api/v1/attachments/{id}/sprites/1#xywh=0,0,160,90

   00:00:05.000 --> 00:00:10.000
   /api/v1/attachments/{id}/sprites/1#xywh=160,0,160,90
   ```

2. **Sprite sheet images** at `/api/v1/attachments/{id}/sprites/{index}` are 5x5 JPEG
   grids (25 thumbnails per sheet, each 160x90 pixels).

3. The `#xywh=x,y,w,h` fragment identifies the pixel region within the sprite sheet for
   that time range.

### Parsing the VTT Map

```ts
interface SpriteEntry {
  startSecs: number;
  endSecs: number;
  imageUrl: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

async function parseSpriteVtt(attachmentId: string): Promise<SpriteEntry[]> {
  const res = await fetch(`/api/v1/attachments/${attachmentId}/thumbnails.vtt`);
  const text = await res.text();
  const entries: SpriteEntry[] = [];

  const blocks = text.split(/\n\n+/).filter(b => b.includes('-->'));
  for (const block of blocks) {
    const lines = block.trim().split('\n');
    const [timeLine, urlLine] = lines;
    const [startStr, endStr] = timeLine.split(' --> ');
    const [imageUrl, frag] = urlLine.split('#xywh=');
    const [x, y, w, h] = frag.split(',').map(Number);

    entries.push({
      startSecs: parseVttTimestamp(startStr),
      endSecs: parseVttTimestamp(endStr),
      imageUrl,
      x, y, w, h,
    });
  }
  return entries;
}

function parseVttTimestamp(ts: string): number {
  const parts = ts.split(':');
  const [h, m] = parts.map(Number);
  const s = parseFloat(parts[2]);
  return h * 3600 + m * 60 + s;
}
```

### Rendering a Seek-Bar Preview

```tsx
function SeekPreview({
  sprites,
  hoverTime,
}: {
  sprites: SpriteEntry[];
  hoverTime: number;
}) {
  const entry = sprites.find(
    s => hoverTime >= s.startSecs && hoverTime < s.endSecs
  );
  if (!entry) return null;

  return (
    <div
      style={{
        width: entry.w,
        height: entry.h,
        backgroundImage: `url(${entry.imageUrl})`,
        backgroundPosition: `-${entry.x}px -${entry.y}px`,
        backgroundSize: `${entry.w * 5}px ${entry.h * 5}px`,
        border: '2px solid white',
        borderRadius: 4,
        boxShadow: '0 2px 8px rgba(0,0,0,0.3)',
      }}
    />
  );
}
```

Sprite sheet images are served with aggressive caching:

```
Cache-Control: public, max-age=31536000, immutable
```

---

## 4. Multi-Memory Routing

When using multiple memory archives, media requests must include the
`X-Fortemi-Memory` header to route to the correct schema.

**The problem:** HTML5 `<video src="...">` and `<audio src="...">` elements issue
browser-level fetch requests that cannot carry custom headers.

**Solution: dual playback mode** (as implemented in HotM).

### Direct Mode (Default)

Used when no custom headers are needed (single memory or default memory):

```tsx
<video src={`/api/v1/attachments/${id}/download`} controls />
```

The browser handles Range requests natively. Best performance.

### Blob Mode (Multi-Memory)

When the `X-Fortemi-Memory` header is required, download the file as a blob with the
correct headers, then create an object URL:

```tsx
async function fetchMediaBlob(
  attachmentId: string,
  memory: string,
  variant?: string,
): Promise<string> {
  const url = variant
    ? `/api/v1/attachments/${attachmentId}/download?variant=${variant}`
    : `/api/v1/attachments/${attachmentId}/download`;

  const res = await fetch(url, {
    headers: { 'X-Fortemi-Memory': memory },
  });
  const blob = await res.blob();
  return URL.createObjectURL(blob);
}

// Usage
const [blobUrl, setBlobUrl] = useState<string>();
useEffect(() => {
  if (activeMemory) {
    fetchMediaBlob(attachmentId, activeMemory, variant).then(setBlobUrl);
  }
  return () => { if (blobUrl) URL.revokeObjectURL(blobUrl); };
}, [attachmentId, activeMemory, variant]);

<video src={activeMemory ? blobUrl : directUrl} controls />
```

> **Important:** Always revoke blob URLs on unmount to prevent memory leaks.

### Subtitle Fetch with Memory Header

Subtitle endpoints also need the memory header:

```ts
const res = await fetch(
  `/api/v1/attachments/${id}/subtitles?format=vtt`,
  { headers: { 'X-Fortemi-Memory': activeMemory } },
);
const vttText = await res.text();
```

---

## 5. Resumable Uploads (TUS Protocol)

Fortemi implements [TUS v1.0.0](https://tus.io/) for reliable large-file uploads with
resume capability. Extensions: Creation, Termination, Expiration.

### Upload Flow

```
1. POST   /tus           → 201 Created (get upload URL in Location header)
2. PATCH  /tus/{id}      → 204 No Content (send chunks, repeat until done)
3. PATCH  /tus/{id}      → 200 OK (final chunk returns the Attachment object)
```

### Step 1: Create Upload Session

```ts
async function createTusUpload(
  noteId: string,
  file: File,
  mediaOptimize = true,
): Promise<{ uploadUrl: string; uploadId: string }> {
  const metadata = [
    `filename ${btoa(file.name)}`,
    `content_type ${btoa(file.type)}`,
  ].join(',');

  const res = await fetch(
    `/api/v1/notes/${noteId}/attachments/tus?media_optimize=${mediaOptimize}`,
    {
      method: 'POST',
      headers: {
        'Tus-Resumable': '1.0.0',
        'Upload-Length': String(file.size),
        'Upload-Metadata': metadata,
      },
    },
  );

  const uploadUrl = res.headers.get('Location')!;
  const uploadId = uploadUrl.split('/').pop()!;
  return { uploadUrl, uploadId };
}
```

### Step 2: Upload Chunks

```ts
async function uploadChunks(
  uploadUrl: string,
  file: File,
  chunkSize = 5 * 1024 * 1024, // 5 MB
  onProgress?: (pct: number) => void,
): Promise<any> {
  let offset = 0;

  while (offset < file.size) {
    const chunk = file.slice(offset, offset + chunkSize);

    const res = await fetch(uploadUrl, {
      method: 'PATCH',
      headers: {
        'Tus-Resumable': '1.0.0',
        'Upload-Offset': String(offset),
        'Content-Type': 'application/offset+octet-stream',
      },
      body: chunk,
    });

    offset = Number(res.headers.get('Upload-Offset'));
    onProgress?.(Math.round((offset / file.size) * 100));

    // Final chunk returns the completed Attachment as JSON
    if (res.status === 200) {
      return await res.json();
    }
  }
}
```

### Step 3: Resume After Interruption

```ts
async function resumeUpload(uploadUrl: string): Promise<number> {
  const res = await fetch(uploadUrl, {
    method: 'HEAD',
    headers: { 'Tus-Resumable': '1.0.0' },
  });
  return Number(res.headers.get('Upload-Offset'));
}

// Resume flow
const currentOffset = await resumeUpload(uploadUrl);
// Then call uploadChunks() starting from currentOffset
```

### Cancelling an Upload

```ts
await fetch(uploadUrl, {
  method: 'DELETE',
  headers: { 'Tus-Resumable': '1.0.0' },
});
```

Upload sessions expire after 24 hours by default (`TUS_UPLOAD_EXPIRY_HOURS`).

---

## 6. Real-Time Processing Events

After upload, Fortemi processes media asynchronously (extraction, transcription, media
optimization, keyframe vision, sprite sheet generation). Use Server-Sent Events to track
progress and know when media is ready.

### Connecting to the SSE Stream

```ts
function subscribeToAttachment(
  attachmentId: string,
  handlers: {
    onExtractionDone?: (data: any) => void;
    onJobProgress?: (data: any) => void;
  },
): EventSource {
  const params = new URLSearchParams({
    types: 'attachment,job',
    entity_id: attachmentId,
  });
  const es = new EventSource(`/api/v1/events?${params}`);

  es.addEventListener('message', (e) => {
    const event = JSON.parse(e.data);
    switch (event.event_type) {
      case 'attachment.extraction_updated':
        handlers.onExtractionDone?.(event.data);
        break;
      case 'job.progress':
        handlers.onJobProgress?.(event.data);
        break;
    }
  });

  return es;
}
```

### Relevant Event Types

| Event | When |
|-------|------|
| `attachment.created` | Attachment record created, processing queued |
| `attachment.extraction_updated` | Extraction complete (transcript, metadata available) |
| `job.queued` | A processing job was queued (extraction, media optimize, etc.) |
| `job.progress` | Progress update with percentage and message |
| `job.completed` | A processing job finished successfully |
| `job.failed` | A processing job failed |

### Polling Fallback

If SSE is not available, poll the attachment metadata endpoint:

```ts
async function waitForProcessing(
  attachmentId: string,
  intervalMs = 3000,
): Promise<any> {
  while (true) {
    const res = await fetch(`/api/v1/attachments/${attachmentId}`);
    const attachment = await res.json();
    if (attachment.status === 'completed' || attachment.status === 'failed') {
      return attachment;
    }
    await new Promise(r => setTimeout(r, intervalMs));
  }
}
```

---

## 7. Error Recovery

HotM implements a graduated recovery strategy. This pattern works for any player.

### Direct-to-Blob Fallback

```tsx
const [mode, setMode] = useState<'direct' | 'blob' | 'error'>('direct');
const [retryCount, setRetryCount] = useState(0);
const MAX_RETRIES = 3;

function handleMediaError(e: React.SyntheticEvent<HTMLVideoElement>) {
  const error = e.currentTarget.error;

  if (mode === 'direct') {
    // Direct URL failed — try blob download
    setMode('blob');
    return;
  }

  if (retryCount < MAX_RETRIES) {
    // Exponential backoff retry
    const delay = Math.pow(2, retryCount) * 1000;
    setTimeout(() => {
      setRetryCount(c => c + 1);
      // Re-trigger blob download
    }, delay);
  } else {
    setMode('error');
  }
}
```

### Playback Position Persistence

Save and restore playback position across sessions:

```ts
const STORAGE_KEY = `playback-${attachmentId}`;

// Save every 5 seconds while playing
useEffect(() => {
  const timer = setInterval(() => {
    if (videoRef.current && !videoRef.current.paused) {
      localStorage.setItem(STORAGE_KEY, String(videoRef.current.currentTime));
    }
  }, 5000);
  return () => clearInterval(timer);
}, [attachmentId]);

// Restore on load
function handleLoadedMetadata() {
  const saved = parseFloat(localStorage.getItem(STORAGE_KEY) ?? '0');
  const duration = videoRef.current?.duration ?? 0;
  if (saved > 1 && saved < duration - 2) {
    videoRef.current!.currentTime = saved;
  }
}

// Clear on ended
function handleEnded() {
  localStorage.removeItem(STORAGE_KEY);
}
```

---

## 8. Complete Example

A production-quality video player with variant selection, captions, transcript panel,
sprite sheet previews, and error recovery.

```tsx
import { useEffect, useRef, useState } from 'react';

interface Attachment {
  id: string;
  content_type: string;
  extracted_metadata?: {
    available_variants?: string[];
    transcript_segments?: TranscriptSegment[];
  };
  has_preview: boolean;
  status: string;
}

const VIDEO_VARIANT_PREFS = ['web_compatible', 'faststart', 'preview_720p'];

export function FortemiVideoPlayer({ attachment }: { attachment: Attachment }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [vttBlobUrl, setVttBlobUrl] = useState<string>();

  const id = attachment.id;
  const variants = attachment.extracted_metadata?.available_variants ?? [];
  const variant = VIDEO_VARIANT_PREFS.find(v => variants.includes(v));
  const segments = attachment.extracted_metadata?.transcript_segments;

  // Build streaming URL
  const src = variant
    ? `/api/v1/attachments/${id}/download?variant=${variant}`
    : `/api/v1/attachments/${id}/download`;

  // Poster image
  const poster = attachment.has_preview
    ? `/api/v1/attachments/${id}/thumbnail`
    : undefined;

  // Generate VTT blob from embedded segments
  useEffect(() => {
    if (!segments?.length) return;
    const url = createVttBlobUrl(segments);
    setVttBlobUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [segments]);

  // Restore playback position
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const onLoaded = () => {
      const saved = parseFloat(
        localStorage.getItem(`playback-${id}`) ?? '0'
      );
      if (saved > 1 && saved < video.duration - 2) {
        video.currentTime = saved;
      }
    };

    const onEnded = () => localStorage.removeItem(`playback-${id}`);

    video.addEventListener('loadedmetadata', onLoaded);
    video.addEventListener('ended', onEnded);
    return () => {
      video.removeEventListener('loadedmetadata', onLoaded);
      video.removeEventListener('ended', onEnded);
    };
  }, [id]);

  // Persist position every 5 seconds
  useEffect(() => {
    const timer = setInterval(() => {
      const video = videoRef.current;
      if (video && !video.paused) {
        localStorage.setItem(`playback-${id}`, String(video.currentTime));
      }
    }, 5000);
    return () => clearInterval(timer);
  }, [id]);

  return (
    <div>
      <video
        ref={videoRef}
        src={src}
        poster={poster}
        controls
        playsInline
        preload="metadata"
        onTimeUpdate={() =>
          setCurrentTime(videoRef.current?.currentTime ?? 0)
        }
        style={{ width: '100%', maxHeight: '70vh' }}
      >
        {/* Server-generated captions */}
        <track
          kind="subtitles"
          src={`/api/v1/attachments/${id}/subtitles?format=vtt`}
          srcLang="en"
          label="English"
        />
        {/* Client-generated captions from metadata */}
        {vttBlobUrl && (
          <track
            kind="subtitles"
            src={vttBlobUrl}
            srcLang="en"
            label="Transcript"
            default
          />
        )}
      </video>

      {/* Interactive transcript */}
      {segments && segments.length > 0 && (
        <TranscriptPanel
          segments={segments}
          currentTime={currentTime}
          onSeek={(t) => {
            if (videoRef.current) videoRef.current.currentTime = t;
          }}
        />
      )}
    </div>
  );
}
```

---

## 9. Authentication

When `REQUIRE_AUTH=true`, all media endpoints require a Bearer token. For direct `<video>`
and `<audio>` elements that cannot carry headers, pass the token as a query parameter on
the SSE endpoint or use the blob download approach.

**API requests (fetch):**

```ts
fetch(`/api/v1/attachments/${id}/download`, {
  headers: { 'Authorization': `Bearer ${token}` },
});
```

**SSE connection with auth:**

```ts
new EventSource(`/api/v1/events?token=${token}&types=attachment`);
```

**For `<video src="...">`**: When auth is required you must use blob mode, since there is
no standard way to add Authorization headers to a `<video>` element's requests.

---

## Attachment Metadata Shape

The attachment object returned by `GET /api/v1/attachments/{id}`:

```json
{
  "id": "uuid",
  "note_id": "uuid",
  "filename": "interview.mp4",
  "content_type": "video/mp4",
  "size_bytes": 52428800,
  "status": "completed",
  "has_preview": true,
  "extraction_strategy": "video_multimodal",
  "extracted_text": "Full transcription text...",
  "ai_description": "A 12-minute interview with two speakers...",
  "extracted_metadata": {
    "duration_secs": 720.5,
    "width": 1920,
    "height": 1080,
    "media_info": {
      "codec": "h264",
      "frame_rate": 30.0,
      "audio_codec": "aac",
      "audio_sample_rate": 48000,
      "bitrate_kbps": 5000
    },
    "available_variants": ["faststart", "web_compatible", "preview_720p", "audio_only"],
    "transcript_segments": [
      {
        "start_secs": 0.0,
        "end_secs": 3.2,
        "text": "Welcome to the interview.",
        "speaker": "Host"
      }
    ],
    "keyframe_descriptions": [
      {
        "frame_index": 0,
        "timestamp_secs": 0.0,
        "description": "A person sitting at a desk with a microphone."
      }
    ]
  },
  "created_at": "2026-02-22T10:30:00Z",
  "updated_at": "2026-02-22T10:35:00Z"
}
```

---

## Caching Behavior

| Endpoint | Cache-Control |
|----------|--------------|
| `/download` | None (fresh content) |
| `/thumbnail` | `public, max-age=86400, immutable` (1 day) |
| `/sprites/{n}` | `public, max-age=31536000, immutable` (1 year) |
| `/thumbnails.vtt` | `public, max-age=86400` (1 day) |
| `/subtitles` | None (may update after re-extraction) |

Thumbnails and sprite sheets are derived from the source video and never change, so they
use immutable caching. Subtitles may change if the transcription is re-run.

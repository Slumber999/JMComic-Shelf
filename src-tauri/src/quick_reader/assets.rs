//! 内置的分享包资源：说明文件与阅读器 HTML

/// 默认的阅读器文件夹名
pub const DEFAULT_READER_DIR_NAME: &str = "快速阅读器";

/// 判断某个目录是不是之前导出的阅读器（用于扫描时跳过）
pub(super) const READER_HTML_FILE: &str = "index.html";

pub(super) const READER_GUIDE_FILE: &str = "使用说明.txt";

pub(super) const READER_GUIDE: &str = "快速阅读器 使用说明\n\n1. 双击本目录下的 index.html，用 Chrome / Edge / Safari 等浏览器打开即可阅读\n2. 手机上：把整个「快速阅读器」文件夹拷到手机，用浏览器打开其中的 index.html\n3. 分享：把整个文件夹压缩后发给别人，对方解压后打开 index.html 就能看\n\n目录结构：\n  快速阅读器/index.html          阅读器本体（已内置漫画清单，无需联网）\n  快速阅读器/漫画名/章节/0001.jpg  解压好的漫画图片\n\n注意：不要只单独发送 index.html，必须把漫画文件夹一起发送。\n";

/// 分享包里 index.html 的内容
/// - 清单内嵌在页面里（file:// 下不能 fetch，所以不能外链 JSON）
/// - 图片用相对路径，双击打开即可阅读
/// - 电脑端固定 600px 宽（和软件内阅读器一致），手机端铺满屏幕宽度
pub(super) const READER_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
<title>漫画快速阅读器</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
  html, body { margin: 0; padding: 0; height: 100%; }
  body { background: #111; color: #e8e8e8; font: 14px/1.5 system-ui, -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif; }
  img { -webkit-user-drag: none; user-select: none; }
  .app { max-width: 600px; margin: 0 auto; min-height: 100%; background: #111; }
  body.mobile .app { max-width: 100%; }

  .page { padding: 12px; }
  h1 { font-size: 16px; margin: 4px 0 12px; font-weight: 600; }
  .sub { color: #8a8a8a; font-size: 12px; }
  .btn { background: #262626; color: #e8e8e8; border: 1px solid #333; border-radius: 6px; padding: 5px 10px; font-size: 13px; cursor: pointer; }
  .btn:hover { background: #333; }
  .btn:disabled { opacity: .4; cursor: default; }
  .btn.on { background: #ff7a00; border-color: #ff7a00; color: #fff; }

  .library { display: flex; flex-direction: column; gap: 8px; }
  .item { display: flex; gap: 10px; padding: 8px; background: #1b1b1b; border: 1px solid #2a2a2a; border-radius: 8px; cursor: pointer; }
  .item:hover { background: #242424; }
  .thumb { width: 64px; height: 88px; flex: 0 0 64px; background: #222; border-radius: 4px; overflow: hidden; display: flex; align-items: center; justify-content: center; color: #666; font-size: 11px; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .info { display: flex; flex-direction: column; justify-content: center; overflow: hidden; }
  .title { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
  .chapters { display: flex; flex-direction: column; gap: 6px; }
  .chapter { display: flex; justify-content: space-between; gap: 10px; padding: 10px; background: #1b1b1b; border: 1px solid #2a2a2a; border-radius: 6px; cursor: pointer; }
  .chapter:hover { background: #242424; }
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .htitle { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .reader { display: flex; flex-direction: column; height: 100vh; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; background: #000; flex-wrap: wrap; }
  .bar .sp { flex: 1; }
  .nowrap { white-space: nowrap; }
  select { background: #262626; color: #e8e8e8; border: 1px solid #333; border-radius: 6px; padding: 5px 6px; font-size: 13px; max-width: 100%; }
  .stage { flex: 1; overflow: auto; position: relative; background: #111; }
  .stage.paged { display: flex; align-items: center; justify-content: center; }
  .stage.scroll { display: flex; flex-direction: column; }
  .sline { width: 100%; }
  .sline img { display: block; width: 100%; height: auto; }
  .stage.paged img { display: block; max-width: 100%; max-height: 100%; object-fit: contain; }
  .zone { position: absolute; top: 0; bottom: 0; width: 28%; }
  .zone.left { left: 0; cursor: w-resize; }
  .zone.right { right: 0; cursor: e-resize; }
  .pager { color: #9a9a9a; }
  input[type=range] { flex: 1; }
</style>
</head>
<body>
<div id="root" class="app"></div>
<script id="manifest" type="application/json">/*__MANIFEST__*/</script>
<script>
(function () {
  var MANIFEST = JSON.parse(document.getElementById('manifest').textContent);
  var root = document.getElementById('root');
  var isMobile = /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent) || window.innerWidth <= 820;
  if (isMobile) document.body.classList.add('mobile');

  var state = { view: 'library', comic: 0, chapter: 0, page: 0, mode: 'scroll' };
  var preloaded = [];
  var lastFlip = 0;

  function esc(s) { return String(s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function url(p) { return p.indexOf('data:') === 0 ? p : encodeURI(p); }
  var imgErrorShown = false;
  function onImgError() {
    if (imgErrorShown) return;
    imgErrorShown = true;
    var tip = document.createElement('div');
    tip.style.cssText = 'position:fixed;left:0;right:0;bottom:0;z-index:99;background:#7a1f1f;color:#fff;padding:10px 14px;font-size:13px;line-height:1.6';
    tip.innerHTML = '图片加载失败：手机浏览器禁止本地网页读取同目录的图片文件。<br>请改用「单文件 HTML」版本（图片已内嵌），或用电脑浏览器打开。';
    document.body.appendChild(tip);
  }
  function comic() { return MANIFEST.comics[state.comic]; }
  function chapter() { return comic().chapters[state.chapter]; }
  function pages() { return chapter().pages; }

  // ---------- 漫画列表 ----------
  function renderLibrary() {
    state.view = 'library';
    var html = '<div class="page"><h1>共 ' + MANIFEST.comics.length + ' 部漫画</h1><div class="library">';
    MANIFEST.comics.forEach(function (c, i) {
      html += '<div class="item" data-comic="' + i + '">' +
        '<div class="thumb">' + (c.cover ? '<img src="' + url(c.cover) + '" loading="lazy" alt="">' : '无封面') + '</div>' +
        '<div class="info"><div class="title">' + esc(c.name) + '</div><div class="sub">' + c.chapters.length + ' 章</div></div>' +
        '</div>';
    });
    root.innerHTML = html + '</div></div>';
    root.querySelectorAll('[data-comic]').forEach(function (el) {
      el.onclick = function () { state.comic = +el.dataset.comic; renderChapters(); };
    });
  }

  // ---------- 章节列表 ----------
  function renderChapters() {
    state.view = 'chapters';
    var c = comic();
    var html = '<div class="page"><div class="head"><button class="btn" id="back">返回</button><span class="htitle">' + esc(c.name) + '</span></div><div class="chapters">';
    c.chapters.forEach(function (ch, i) {
      html += '<div class="chapter" data-chapter="' + i + '"><span>' + esc(ch.title) + '</span><span class="sub">' + ch.pages.length + ' 页</span></div>';
    });
    root.innerHTML = html + '</div></div>';
    var back = root.querySelector('#back');
    if (MANIFEST.comics.length > 1) { back.onclick = renderLibrary; } else { back.style.display = 'none'; }
    root.querySelectorAll('[data-chapter]').forEach(function (el) {
      el.onclick = function () { state.chapter = +el.dataset.chapter; state.page = 0; renderReader(); };
    });
  }

  // ---------- 阅读器 ----------
  function renderReader() {
    state.view = 'reader';
    var c = comic();
    var ch = chapter();
    var options = c.chapters.map(function (x, i) {
      return '<option value="' + i + '"' + (i === state.chapter ? ' selected' : '') + '>' + esc(x.title) + '</option>';
    }).join('');

    root.innerHTML =
      '<div class="reader">' +
        '<div class="bar">' +
          '<button class="btn" id="back">返回</button>' +
          '<span class="htitle">' + esc(c.name) + '</span>' +
          '<span class="sp"></span>' +
          '<span class="nowrap pager"><span id="cur">' + (state.page + 1) + '</span> / ' + ch.pages.length + '</span>' +
        '</div>' +
        '<div class="bar">' +
          '<button class="btn" id="prevch"' + (state.chapter === 0 ? ' disabled' : '') + '>上一章</button>' +
          '<select id="chselect">' + options + '</select>' +
          '<button class="btn" id="nextch"' + (state.chapter === c.chapters.length - 1 ? ' disabled' : '') + '>下一章</button>' +
          '<span class="sp"></span>' +
          '<button class="btn' + (state.mode === 'scroll' ? ' on' : '') + '" id="mscroll">上下滑动</button>' +
          '<button class="btn' + (state.mode === 'paged' ? ' on' : '') + '" id="mpaged">左右翻页</button>' +
        '</div>' +
        '<div class="stage" id="stage"></div>' +
        '<div class="bar" id="bottom" style="display:none"></div>' +
      '</div>';

    var back = root.querySelector('#back');
    back.onclick = function () { if (MANIFEST.comics.length > 1) { renderChapters(); } else { renderChapters(); } };
    root.querySelector('#chselect').onchange = function (e) {
      state.chapter = +e.target.value; state.page = 0; renderReader();
    };
    root.querySelector('#prevch').onclick = function () { goChapter(state.chapter - 1); };
    root.querySelector('#nextch').onclick = function () { goChapter(state.chapter + 1); };
    root.querySelector('#mscroll').onclick = function () { setMode('scroll'); };
    root.querySelector('#mpaged').onclick = function () { setMode('paged'); };
    renderStage();
  }

  function setMode(mode) {
    if (state.mode === mode) return;
    state.mode = mode; state.page = 0;
    renderReader();
  }

  function goChapter(index) {
    if (index < 0 || index >= comic().chapters.length) return;
    state.chapter = index; state.page = 0;
    renderReader();
  }

  function renderStage() {
    var stage = root.querySelector('#stage');
    var list = pages();
    stage.className = 'stage ' + state.mode;
    stage.scrollTop = 0;
    preloaded = [];

    if (state.mode === 'scroll') {
      var html = '';
      list.forEach(function (p, i) {
        html += '<div class="sline" data-page="' + i + '"><img src="' + url(p) + '" loading="lazy" decoding="async" alt="" onerror="window.__qrImgError && window.__qrImgError()"></div>';
      });
      stage.innerHTML = html;
      stage.onscroll = onScroll;
      stage.onwheel = null;
    } else {
      stage.innerHTML = '<img id="pimg" src="' + url(list[state.page]) + '" decoding="async" alt="" onerror="window.__qrImgError && window.__qrImgError()">' +
        '<div class="zone left" id="zl"></div><div class="zone right" id="zr"></div>';
      root.querySelector('#zl').onclick = prevPage;
      root.querySelector('#zr').onclick = nextPage;
      stage.onscroll = null;
      stage.onwheel = onWheel;
      prefetch();
    }

    renderBottom();
    updateIndicator();
  }

  function renderBottom() {
    var bottom = root.querySelector('#bottom');
    if (state.mode !== 'paged' || pages().length <= 1) { bottom.style.display = 'none'; return; }
    bottom.style.display = 'flex';
    bottom.innerHTML = '<button class="btn" id="pprev">‹</button>' +
      '<input type="range" id="pslider" min="0" max="' + (pages().length - 1) + '" value="' + state.page + '">' +
      '<button class="btn" id="pnext">›</button>';
    root.querySelector('#pprev').onclick = prevPage;
    root.querySelector('#pnext').onclick = nextPage;
    root.querySelector('#pslider').oninput = function (e) { goPage(+e.target.value); };
  }

  function goPage(index) {
    var max = pages().length - 1;
    if (index < 0) index = 0;
    if (index > max) index = max;
    state.page = index;
    var image = root.querySelector('#pimg');
    if (image) { image.src = url(pages()[index]); }
    var slider = root.querySelector('#pslider');
    if (slider) { slider.value = String(index); }
    var stage = root.querySelector('#stage');
    if (stage && state.mode === 'paged') { stage.scrollTop = 0; }
    updateIndicator();
    prefetch();
    save();
  }

  function nextPage() { if (state.page < pages().length - 1) goPage(state.page + 1); else goChapter(state.chapter + 1); }
  function prevPage() { if (state.page > 0) goPage(state.page - 1); else goChapter(state.chapter - 1); }

  function updateIndicator() {
    var cur = root.querySelector('#cur');
    if (cur) cur.textContent = String(state.page + 1);
  }

  function prefetch() {
    if (state.mode !== 'paged') return;
    var list = pages();
    preloaded = [];
    [1, 2, -1].forEach(function (offset) {
      var index = state.page + offset;
      if (index < 0 || index >= list.length) return;
      var image = new Image();
      image.src = url(list[index]);
      preloaded.push(image);
    });
    save();
  }

  function onScroll() {
    var stage = root.querySelector('#stage');
    if (!stage) return;
    var marker = stage.scrollTop + stage.clientHeight * 0.4;
    var lines = stage.querySelectorAll('[data-page]');
    var current = 0;
    for (var i = 0; i < lines.length; i++) {
      if (lines[i].offsetTop <= marker) { current = +lines[i].dataset.page; } else { break; }
    }
    if (current !== state.page) { state.page = current; updateIndicator(); save(); }
  }

  function onWheel(event) {
    var stage = root.querySelector('#stage');
    if (!stage) return;
    var now = Date.now();
    if (now - lastFlip < 160) { return; }
    var atBottom = stage.scrollTop + stage.clientHeight >= stage.scrollHeight - 2;
    var atTop = stage.scrollTop <= 2;
    if (event.deltaY > 0 && atBottom) { lastFlip = now; nextPage(); }
    else if (event.deltaY < 0 && atTop) { lastFlip = now; prevPage(); }
  }

  function save() {
    try { localStorage.setItem('qr:last', JSON.stringify({ comic: state.comic, chapter: state.chapter, page: state.page, mode: state.mode })); } catch (e) {}
  }

  function restore() {
    try {
      var raw = localStorage.getItem('qr:last');
      if (!raw) return;
      var saved = JSON.parse(raw);
      if (typeof saved.comic === 'number' && saved.comic < MANIFEST.comics.length) state.comic = saved.comic;
      if (saved.mode === 'paged' || saved.mode === 'scroll') state.mode = saved.mode;
      var chapterCount = MANIFEST.comics[state.comic].chapters.length;
      if (typeof saved.chapter === 'number' && saved.chapter < chapterCount) state.chapter = saved.chapter;
    } catch (e) {}
  }

  window.__qrImgError = onImgError;

  document.addEventListener('keydown', function (event) {
    if (state.view !== 'reader') { if (event.key === 'Escape') renderChapters(); return; }
    var stage = root.querySelector('#stage');
    var step = (stage ? stage.clientHeight : window.innerHeight) * 0.9;
    if (event.key === 'ArrowRight') { nextPage(); }
    else if (event.key === 'ArrowLeft') { prevPage(); }
    else if (event.key === 'ArrowDown' || event.key === 'PageDown' || event.key === ' ') { if (stage) stage.scrollBy({ top: step }); }
    else if (event.key === 'ArrowUp' || event.key === 'PageUp') { if (stage) stage.scrollBy({ top: -step }); }
    else if (event.key === 'm' || event.key === 'M') { setMode(state.mode === 'paged' ? 'scroll' : 'paged'); }
    else if (event.key === 'Escape') { renderChapters(); }
    else { return; }
    event.preventDefault();
  });

  restore();
  if (MANIFEST.comics.length === 1) { renderChapters(); } else { renderLibrary(); }
})();
</script>
</body>
</html>
"##;

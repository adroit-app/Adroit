(function(){
var viewer=null,loaded={};
function draw(){if(viewer&&window.ArtoRenderer)ArtoRenderer.render(viewer);}
// The page takes the viewer's background, which the theme's tokens give the
// viewer alone, so the frame's own shows nowhere: not under the scrollbar.
function paint(theme){
  if(!theme)return;
  document.documentElement.setAttribute('data-adroit-theme',theme);
  if(window.ArtoRenderer)ArtoRenderer.setTheme(theme);
  if(viewer)document.documentElement.style.backgroundColor=getComputedStyle(viewer).backgroundColor;
}
// Bundles are loaded one after another, in the order given, as the head
// loads them: each registers itself on the runtime loaded before it.
function load(urls,then){
  var next=urls.filter(function(u){return !loaded[u];});
  (function step(i){
    if(i>=next.length)return then();
    var s=document.createElement('script');s.src=next[i];
    s.onload=s.onerror=function(){loaded[next[i]]=1;step(i+1);};
    document.head.appendChild(s);
  })(0);
}
// The document's links to other files are opened by the window; its own
// places are scrolled to here, as the frame navigates nowhere.
window.handleMarkdownLinkClick=function(path){
  parent.postMessage({adroitViewerLink:1,path:String(path)},'*');
};
document.addEventListener('click',function(e){
  var a=e.target&&e.target.closest&&e.target.closest('a[href^="#"]');
  if(!a)return;
  e.preventDefault();
  var id=decodeURIComponent(a.getAttribute('href').slice(1));
  var to=id?document.getElementById(id):null;
  if(to)to.scrollIntoView();else if(!id)scrollTo(0,0);
});
addEventListener('message',function(e){
  if(e.source!==parent)return;var m=e.data;if(!m||m.adroitViewer!==1)return;
  if(m.theme)paint(m.theme);
  if(typeof m.markdown==='string')load(m.scripts||[],function(){
    var body=viewer&&viewer.querySelector('.markdown-body');
    if(body){body.innerHTML=m.markdown;draw();}
  });
});
document.addEventListener('DOMContentLoaded',function(){
  Array.prototype.forEach.call(document.scripts,function(s){if(s.src)loaded[s.src]=1;});
  viewer=document.querySelector('.markdown-viewer');
  paint(document.documentElement.getAttribute('data-adroit-theme'));
  draw();
});
})();

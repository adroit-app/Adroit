(function(){
// The pane shown: Conversation, Files changed or Checks. It stays chosen when
// what is read again replaces the page's contents.
var shown='conversation';
function show(){
  Array.prototype.forEach.call(document.querySelectorAll('.gh-tab[data-pane]'),function(t){
    t.classList.toggle('on',t.getAttribute('data-pane')===shown);
  });
  Array.prototype.forEach.call(document.querySelectorAll('.gh-pane[data-pane]'),function(p){
    p.hidden=p.getAttribute('data-pane')!==shown;
  });
}
document.addEventListener('click',function(e){
  var tab=e.target&&e.target.closest&&e.target.closest('.gh-tab[data-pane]');
  if(tab){shown=tab.getAttribute('data-pane');show();scrollTo(0,0);return;}
  // A link to an address is opened by the window, in the browser: the frame
  // navigates nowhere. The document's own places are scrolled to by the
  // document's script.
  var a=e.target&&e.target.closest&&e.target.closest('a[href]');
  if(!a)return;
  var href=a.getAttribute('href')||'';
  if(href.charAt(0)==='#')return;
  e.preventDefault();
  e.stopPropagation();
  if(/^https?:\/\//i.test(a.href))parent.postMessage({adroitViewerOpen:1,url:a.href},'*');
},true);
document.addEventListener('DOMContentLoaded',function(){
  var thread=document.querySelector('.gh-thread');
  if(thread)new MutationObserver(show).observe(thread,{childList:true});
});
})();

(function(){
// A link to an address is opened by the window, in the browser: the frame
// navigates nowhere. The document's own places are scrolled to by the
// document's script.
document.addEventListener('click',function(e){
  var a=e.target&&e.target.closest&&e.target.closest('a[href]');
  if(!a)return;
  var href=a.getAttribute('href')||'';
  if(href.charAt(0)==='#')return;
  e.preventDefault();
  e.stopPropagation();
  if(/^https?:\/\//i.test(a.href))parent.postMessage({adroitViewerOpen:1,url:a.href},'*');
},true);
})();

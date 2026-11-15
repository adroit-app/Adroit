(function(){
addEventListener('message',function(e){
  if(e.source!==parent)return;var m=e.data;if(!m||m.adroitViewer!==1)return;
  var el=document.scrollingElement||document.documentElement;
  if(m.zoom)document.documentElement.style.zoom=m.zoom;
  if(typeof m.top==='number')el.scrollTo(0,m.top);
  if(m.scroll==='top')el.scrollTo(0,0);
  else if(m.scroll==='bottom')el.scrollTo(0,el.scrollHeight);
  else if(m.lines)el.scrollBy(0,m.lines*40);
  else if(m.pages)el.scrollBy(0,m.pages*innerHeight/2);
});
addEventListener('keydown',function(e){
  var t=e.target;
  if(t&&(t.isContentEditable||/^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName)))return;
  parent.postMessage({adroitViewerKey:1,key:e.key,code:e.code,ctrlKey:e.ctrlKey,
    metaKey:e.metaKey,altKey:e.altKey,shiftKey:e.shiftKey},'*');
},true);
// Where the page is scrolled to, for the viewer to come back to.
var told=false;
addEventListener('scroll',function(){
  if(told)return;told=true;
  requestAnimationFrame(function(){
    told=false;
    var el=document.scrollingElement||document.documentElement;
    parent.postMessage({adroitViewerTop:1,top:el.scrollTop},'*');
  });
},{passive:true});
})();

(function(){
var viewer=null;
function draw(){if(viewer&&window.ArtoRenderer)ArtoRenderer.render(viewer);}
document.addEventListener('DOMContentLoaded',function(){
  viewer=document.querySelector('.markdown-viewer');draw();
});
})();

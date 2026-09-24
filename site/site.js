/* Same dependency-free interaction language as the sibling SSDownload site. */
(function(){'use strict';
  var root=document.documentElement;
  var langKey='isolmass.lang';
  var reduced=window.matchMedia&&window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var stored='tr';
  try{stored=localStorage.getItem(langKey)||'tr'}catch(_){}
  var query=new URLSearchParams(location.search).get('lang');
  function choose(lang,save){
    if(lang!=='tr'&&lang!=='en')lang='tr';
    root.lang=lang;root.setAttribute('data-lang',lang);
    document.querySelectorAll('[data-set-lang]').forEach(function(button){button.setAttribute('aria-pressed',String(button.dataset.setLang===lang))});
    var title=root.getAttribute('data-title-'+lang);
    if(title)document.title=title;
    var description=root.getAttribute('data-desc-'+lang);
    var meta=document.querySelector('meta[name="description"]');
    if(description&&meta)meta.content=description;
    if(save)try{localStorage.setItem(langKey,lang)}catch(_){}
    refreshCaption();
  }
  document.querySelectorAll('[data-set-lang]').forEach(function(button){button.addEventListener('click',function(){choose(button.dataset.setLang,true)})});
  var header=document.querySelector('.site-header');
  function onScroll(){if(header)header.setAttribute('data-scrolled',String(scrollY>4));var bar=document.querySelector('[data-read-progress]');if(bar){var end=document.documentElement.scrollHeight-innerHeight;bar.style.transform='scaleX('+(end>0?Math.min(1,scrollY/end):0)+')'}}
  addEventListener('scroll',onScroll,{passive:true});onScroll();
  var toggle=document.querySelector('[data-nav-toggle]'),nav=document.querySelector('[data-nav]');
  if(toggle&&nav){function open(state){nav.classList.toggle('is-open',state);toggle.setAttribute('aria-expanded',String(state))}toggle.addEventListener('click',function(){open(toggle.getAttribute('aria-expanded')!=='true')});nav.addEventListener('click',function(event){if(event.target.closest('a'))open(false)});document.addEventListener('keydown',function(event){if(event.key==='Escape'&&toggle.getAttribute('aria-expanded')==='true'){open(false);toggle.focus()}});addEventListener('resize',function(){if(innerWidth>760)open(false)},{passive:true})}
  var carousel=document.querySelector('[data-carousel]');var index=0,slides=[],track=null,caption=null,dots=null;
  function refreshCaption(){if(!slides.length||!caption)return;caption.textContent=slides[index].getAttribute('data-label-'+root.lang)||'';dots.querySelectorAll('button').forEach(function(dot,i){dot.setAttribute('aria-current',String(i===index));dot.setAttribute('aria-label',(root.lang==='en'?'Show slide ':'Görünüm ')+(i+1))})}
  function go(next){if(!slides.length)return;index=(next+slides.length)%slides.length;track.style.transform='translateX(-'+(index*100)+'%)';refreshCaption()}
  if(carousel){track=carousel.querySelector('[data-track]');caption=carousel.querySelector('[data-caption]');dots=carousel.querySelector('[data-dots]');slides=Array.from(carousel.querySelectorAll('[data-slide]'));
    slides.forEach(function(_,i){var dot=document.createElement('button');dot.type='button';dot.addEventListener('click',function(){go(i)});dots.appendChild(dot)});
    carousel.querySelector('[data-prev]').addEventListener('click',function(){go(index-1)});carousel.querySelector('[data-next]').addEventListener('click',function(){go(index+1)});
    var stage=carousel.querySelector('[data-stage]');stage.addEventListener('keydown',function(event){if(event.key==='ArrowRight'||event.key==='ArrowLeft'){event.preventDefault();go(index+(event.key==='ArrowRight'?1:-1))}});
  }
  choose(query==='tr'||query==='en'?query:stored,false);
  if(!reduced&&'IntersectionObserver'in window){var observer=new IntersectionObserver(function(entries){entries.forEach(function(entry){if(entry.isIntersecting){entry.target.classList.add('is-in');observer.unobserve(entry.target)}})},{threshold:.04});document.querySelectorAll('.reveal').forEach(function(item){observer.observe(item)})}else{document.querySelectorAll('.reveal').forEach(function(item){item.classList.add('is-in')})}
  root.classList.add('js');
})();

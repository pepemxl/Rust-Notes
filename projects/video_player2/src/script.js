document.addEventListener('DOMContentLoaded', function() {
    const video = document.getElementById('video');
    const prevFrameBtn = document.getElementById('prevFrame');
    const nextFrameBtn = document.getElementById('nextFrame');
    const play2xBtn = document.getElementById('play2x');
    const play5xBtn = document.getElementById('play5x');
    const videoSelector = document.getElementById('videoSelector');
    const frameRate = 24.75;
    const frameTime = 1/frameRate;
    prevFrameBtn.addEventListener('click', ()=>{
        video.pause();
        video.currentTime = Math.max(0, video.currentTime - frameTime);
    });
    nextFrameBtn.addEventListener('click', ()=>{
        video.pause();
        video.currentTime = Math.min(video.duration, video.currentTime + frameTime);
    });
    play2xBtn.addEventListener('click', ()=>{
        video.playbackRate = 2.0;
        video.play();
    });
    play5xBtn.addEventListener('click', ()=>{
        video.playbackRate = 5.0;
        video.play();
    });
    videoSelector.addEventListener('click', ()=>{
        const selectedVideo = event.target.value;
        video.querySelector('source').src = selectedVideo;
        video.onload();
    })
})


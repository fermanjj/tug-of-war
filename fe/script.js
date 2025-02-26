const socket = new WebSocket('ws://localhost:3000/ws');
const flag = document.getElementById('flag');
const leftPulls = document.getElementById('left-pulls');
const rightPulls = document.getElementById('right-pulls');
const scoreDisplay = document.getElementById('score');
const pullLeftBtn = document.getElementById('pull-left');
const pullRightBtn = document.getElementById('pull-right');
const minimap = document.getElementById('minimap');
const minimapFlag = document.getElementById('minimap-flag');
const minimapInset = document.getElementById('minimap-inset');
const minimapInsetFlag = document.getElementById('minimap-inset-flag');
const pulse = document.getElementById('pulse');
const messageLog = document.getElementById('message-log');
const userCount = document.getElementById('user-count');

let position = 0;
let prevLeftPulls = 0;
let prevRightPulls = 0;
const TOTAL_STEPS = 10000000;
const INSET_RANGE = 100000; // Show ±50k steps in inset

socket.onmessage = (event) => {
    const data = JSON.parse(event.data);
    const newPosition = data.position;
    const newLeftPulls = data.left_pulls;
    const newRightPulls = data.right_pulls;

    let pullSide = '';
    if (newLeftPulls > prevLeftPulls) {
        pullSide = 'Left';
    } else if (newRightPulls > prevRightPulls) {
        pullSide = 'Right';
    }

    position = newPosition;
    prevLeftPulls = newLeftPulls;
    prevRightPulls = newRightPulls;
    leftPulls.textContent = `Left Pulls: ${newLeftPulls.toLocaleString()}`;
    rightPulls.textContent = `Right Pulls: ${newRightPulls.toLocaleString()}`;
    userCount.textContent = `Players: ${data.active_users}`;

    const flagPos = ((position + TOTAL_STEPS) / (2 * TOTAL_STEPS)) * 100;
    flag.style.left = `${flagPos}%`;
    minimapFlag.style.left = `${flagPos}%`;

    // Inset: Map ±50k steps to 0-100%
    const insetPos = ((position + INSET_RANGE) / (2 * INSET_RANGE)) * 100;
    minimapInsetFlag.style.left = `${Math.max(0, Math.min(100, insetPos))}%`;

    const score = Math.abs(position);
    const percentage = (Math.abs(position) / TOTAL_STEPS) * 100;
    const leadingSide = position >= 0 ? 'Right' : 'Left';
    scoreDisplay.textContent = `Score: ${score.toLocaleString()} (${percentage.toFixed(1)}% to ${leadingSide})`;

    pulse.style.left = `${flagPos}%`;
    pulse.style.width = '20px';
    pulse.style.height = '20px';
    pulse.style.opacity = '0.8';
    setTimeout(() => {
        pulse.style.width = '100px';
        pulse.style.height = '100px';
        pulse.style.opacity = '0';
    }, 10);
    setTimeout(() => {
        pulse.style.width = '0';
        pulse.style.height = '0';
        pulse.style.opacity = '0.8';
    }, 500);

    if (pullSide) {
        const msg = document.createElement('div');
        msg.className = 'message';
        msg.textContent = `${pullSide} pulled! ${score.toLocaleString()} steps`;
        messageLog.insertBefore(msg, messageLog.firstChild);
        setTimeout(() => {
            msg.classList.add('fade');
            setTimeout(() => msg.remove(), 500);
        }, 500);
    }

    if (Math.abs(position) >= TOTAL_STEPS) {
        alert(position > 0 ? "Right Wins!" : "Left Wins!");
        position = 0;
        prevLeftPulls = 0;
        prevRightPulls = 0;
    }
};

pullLeftBtn.onclick = () => socket.send(JSON.stringify({action: 'pull', direction: 'left'}));
pullRightBtn.onclick = () => socket.send(JSON.stringify({action: 'pull', direction: 'right'}));

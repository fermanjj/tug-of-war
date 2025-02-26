const socket = new WebSocket('ws://localhost:3000/ws');
const flag = document.getElementById('flag');
const leftPulls = document.getElementById('left-pulls');
const rightPulls = document.getElementById('right-pulls');
const scoreDisplay = document.getElementById('score');
const pullLeftBtn = document.getElementById('pull-left');
const pullRightBtn = document.getElementById('pull-right');
const minimap = document.getElementById('minimap');
const minimapFlag = document.getElementById('minimap-flag');
const pulse = document.getElementById('pulse');
const messageLog = document.getElementById('message-log');

let position = 0;
let prevLeftPulls = 0; // Track previous pull counts
let prevRightPulls = 0;
const TOTAL_STEPS = 10000000;

// Update UI based on server data
socket.onmessage = (event) => {
    const data = JSON.parse(event.data);
    const newPosition = data.position;
    const newLeftPulls = data.left_pulls;
    const newRightPulls = data.right_pulls;

    // Determine which side pulled
    let pullSide = '';
    if (newLeftPulls > prevLeftPulls) {
        pullSide = 'Left';
    } else if (newRightPulls > prevRightPulls) {
        pullSide = 'Right';
    }

    // Update state
    position = newPosition;
    prevLeftPulls = newLeftPulls;
    prevRightPulls = newRightPulls;
    leftPulls.textContent = `Left Pulls: ${newLeftPulls.toLocaleString()}`;
    rightPulls.textContent = `Right Pulls: ${newRightPulls.toLocaleString()}`;

    // Move flag and minimap flag
    const flagPos = ((position + TOTAL_STEPS) / (2 * TOTAL_STEPS)) * 100;
    flag.style.left = `${flagPos}%`;
    minimapFlag.style.left = `${flagPos}%`;

    // Update score and percentage
    const score = Math.abs(position);
    const percentage = (Math.abs(position) / TOTAL_STEPS) * 100;
    const leadingSide = position >= 0 ? 'Right' : 'Left';
    scoreDisplay.textContent = `Score: ${score.toLocaleString()} (${percentage.toFixed(1)}% to ${leadingSide})`;

    // Pulse animation
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

    // Add message to log if a pull occurred
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

    // Check win
    if (Math.abs(position) >= TOTAL_STEPS) {
        alert(position > 0 ? "Right Wins!" : "Left Wins!");
        position = 0;
        prevLeftPulls = 0;
        prevRightPulls = 0;
    }
};

// Button clicks
pullLeftBtn.onclick = () => socket.send(JSON.stringify({action: 'pull', direction: 'left'}));
pullRightBtn.onclick = () => socket.send(JSON.stringify({action: 'pull', direction: 'right'}));

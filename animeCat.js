// animeCat.js
// miau!

window.AnimeCat = class AnimeCat {
  /**
   * @param {HTMLElement} container
   * @param {Object}      [options]
   */
  constructor(container, options = {}) {
    this.container     = container;
    this.images        = Object.assign({
      default:     'default.png',
      eyesClosed:  'eyes_closed.png',
      blink:       'blink.png',
      mouthOpen:   'mouth_open.png',
      joy:         'joy.png' 
    }, options.images);
    this.blinkMin      = options.blinkMin      ?? 5000;
    this.blinkMax      = options.blinkMax      ?? 15000;
    this.blinkDuration = options.blinkDuration ?? 175;
    this.talkInterval  = options.talkInterval  ?? 300;

    this._isSpeaking     = false;
    this._blinkTimeout   = null;
    this._talkIntervalId = null;
    this._speechTimeout  = null;
    this._mouthOpen      = false;
    this._pettingActive = false;

    this._createElements();
    this._bindMouseHold();
    this._startBlinking();
  }

  _createElements() {
    // Flexbox: Katze und Bubble nebeneinander
    this.wrapper = document.createElement('div');
    Object.assign(this.wrapper.style, {
      position:      'relative',
      display:       'flex',
      flexDirection: 'row',
      alignItems:    'flex-end',
      minHeight:     '60px',
      minWidth:      '180px', // Passe an falls du willst
      zIndex:        '1'
    });

    // --- Cat image ---
    this.img = document.createElement('img');
    this.img.src = this.images.default;
    this.img.draggable = false;
    this.img.style.userSelect = 'none';
    this.img.style.webkitUserSelect = 'none';
    this.img.style.MozUserSelect = 'none';
    this.img.style.msUserSelect = 'none';
    this.img.style.webkitUserDrag = 'none';
    this.img.style.width = '60px';
    this.img.style.height = '50px';
    this.img.style.zIndex = '2';

    // --- Speech bubble ---
    this.bubble = document.createElement('div');
    Object.assign(this.bubble.style, {
      position:      'relative', // jetzt relativ zur wrapper-Flexbox
      marginLeft:    '14px',
      marginBottom:  '11px', 
      padding:       '10px 16px',
      background:    'white',
      border:        '1px solid #ccc',
      borderRadius:  '16px 16px 16px 16px',
      boxShadow:     '0 2px 8px rgba(0,0,0,0.18)',
      opacity:       '0',
      transition:    'opacity 0.3s',
      maxWidth:      '240px',
      wordBreak:     'break-word',
      fontFamily:    'sans-serif',
      fontSize:      '15px',
      color:         '#333',
      pointerEvents: 'none',
      minHeight:     '30px',
      zIndex:        '10',
      overflowWrap:  'anywhere',
      display:       'flex',
      alignItems:    'center'
    });

    // --- Bubble "Tail" (kleines Dreieck) ---
    this.bubblePointer = document.createElement('div');
    Object.assign(this.bubblePointer.style, {
      position:      'absolute',
      left:          '-13px',
      bottom:        '12px',
      width:         '0',
      height:        '0',
      borderTop:     '8px solid transparent',
      borderBottom:  '8px solid transparent',
      borderRight:   '13px solid #fff',
      filter:        'drop-shadow(-2px 1px 2px rgba(0,0,0,0.10))',
      zIndex:        '11'
    });
    this.bubble.appendChild(this.bubblePointer);

    // --- Zusammenbauen ---
    this.wrapper.appendChild(this.img);
    this.wrapper.appendChild(this.bubble);
    this.container.appendChild(this.wrapper);

    // Optional: Passe die Bubble-Dynamik an das Fenster an
    // window.addEventListener('resize', () => this._adaptBubbleWidth());
    // this._adaptBubbleWidth();
  }

  // Passe Bubble-Breite an verfügbaren Platz an (optional)
  _adaptBubbleWidth() {
    const rect = this.wrapper.getBoundingClientRect();
    const parentRect = this.container.getBoundingClientRect();
    const spaceRight = parentRect.right - rect.right - 14;
    this.bubble.style.maxWidth = Math.max(140, Math.min(spaceRight, 320)) + 'px';
  }

  _startBlinking() {
    const delay = this.blinkMin + Math.random() * (this.blinkMax - this.blinkMin);
    this._blinkTimeout = setTimeout(() => {
      if (!this._isSpeaking) {
        this.img.src = this.images.eyesClosed;
        setTimeout(() => {
          if (!this._pettingActive) {
            this.img.src = this.images.default;
          }
          this._startBlinking();
        }, this.blinkDuration);
      } else {
        this._startBlinking();
      }
    }, delay);
  }

_bindMouseHold() {
  let holdTimer = null;
  let joyTimeout = null;
  let joyActive = false;
  let mouseDown = false;        // <---- NEU! Initialisieren
  let mouseDownAt = null;       // <---- NEU!
  let lastPos = null;           // <---- NEU!
  let moveDist = 0;             // <---- NEU!

  const CAT_TOLERANCE = 15;
  const MOVE_THRESHOLD = 350;

  const isMouseNearCat = (e) => {
    const rect = this.img.getBoundingClientRect();
    return (
      e.clientX >= rect.left - CAT_TOLERANCE &&
      e.clientX <= rect.right + CAT_TOLERANCE &&
      e.clientY >= rect.top - CAT_TOLERANCE &&
      e.clientY <= rect.bottom + CAT_TOLERANCE
    );
  };

  const closeEyes = () => { this.img.src = this.images.eyesClosed; };
  const reopenEyes = () => {
    if (!joyActive && !this._isSpeaking && !this._pettingActive) this.img.src = this.images.default;
  };

  this.img.addEventListener('mousedown', (e) => {
    if (this._isSpeaking || joyActive) return;
    if (!isMouseNearCat(e)) return;
    this._pettingActive = true;

    mouseDown = true;
    mouseDownAt = Date.now();
    lastPos = { x: e.clientX, y: e.clientY };
    moveDist = 0;
    closeEyes();
    clearTimeout(this._blinkTimeout); // Blinzeln pausieren

    // Bewegung tracken
    function onMove(ev) {
      if (!mouseDown) return;
      if (!isMouseNearCat(ev)) {
        cleanup();
        reopenEyes();
        mouseDown = false;
        this._startBlinking();
        return;
      }
      if (lastPos) {
        const dx = ev.clientX - lastPos.x;
        const dy = ev.clientY - lastPos.y;
        moveDist += Math.sqrt(dx * dx + dy * dy);
        lastPos = { x: ev.clientX, y: ev.clientY };
      }
    }

    const onMoveBound = onMove.bind(this);

    holdTimer = setTimeout(() => {
      if (!mouseDown) return; // Schon abgebrochen
      
      // Joy-Bedingung
      if (moveDist >= MOVE_THRESHOLD) {
        cleanup();
        joyActive = true;
        this._runJoyAnimation(() => {
          joyActive = false;
          reopenEyes();
          this._startBlinking();
        });
        mouseDown = false;
      } else {
        // Trostpreis: Augen auf, dann mouth_open
        cleanup();
        mouseDown = false;
        reopenEyes();
        const heldFor = Date.now() - mouseDownAt;
        const mouthOpenTime = Math.max(heldFor - 1000, 0);
        if (mouthOpenTime > 0) {
          this.img.src = this.images.mouthOpen || this.images.default;
          this._startBlinking();
          setTimeout(() => {
            if (!joyActive && !this._isSpeaking && !this._pettingActive) this.img.src = this.images.default;
          }, mouthOpenTime);
        }
        else {
          this._startBlinking();
        }
      }
    }, 4000);

    function onUp() {
      if (!mouseDown) return;
      cleanup();
      const heldFor = Date.now() - mouseDownAt;
      if (heldFor >= 4000) return; // already handled by timer above
      // Joy: wenn beide Bedingungen erfüllt
      if (heldFor >= 4000 && moveDist >= MOVE_THRESHOLD) {
        joyActive = true;
        this._runJoyAnimation(() => {
          joyActive = false;
          reopenEyes();
          this._startBlinking();
        });
      }
      // Trostpreis: siehe oben
      else if (heldFor > 1000) {
        reopenEyes();
        const mouthOpenTime = Math.max(heldFor - 1000, 0);
        if (mouthOpenTime > 0) {
          this.img.src = this.images.mouthOpen || this.images.default;
          this._startBlinking();
          setTimeout(() => {
            if (!joyActive && !this._isSpeaking && !this._pettingActive) this.img.src = this.images.default;
          }, mouthOpenTime);
        }
        else {
          this._startBlinking();
        }
      } else {
        reopenEyes();
        this._startBlinking();
      }
      mouseDown = false;
    }

    const onUpBound = onUp.bind(this);

    const cleanup = () => {
      window.removeEventListener('mousemove', onMoveBound);
      window.removeEventListener('mouseup', onUpBound);
      if (holdTimer) clearTimeout(holdTimer);
      holdTimer = null;
      this._pettingActive = false; // Korrekt!
    };
    window.addEventListener('mousemove', onMoveBound);
    window.addEventListener('mouseup', onUpBound);

  });
}

  _runJoyAnimation(onFinish) {
    const wrapper = this.wrapper;
    const img     = this.img;
    const origTransition = wrapper.style.transition;
    const origTransform  = wrapper.style.transform;

    // Bild auf joy.png setzen:
    img.src = this.images.joy || this.images.default;

    // Nach oben springen und nach RECHTS rotieren:
    wrapper.style.transition = 'transform 0.6s cubic-bezier(.19,1,.22,1)';
    wrapper.style.transform  = 'translateY(-40px) rotate(12deg)'; 

    this._spawnHearts(12);

    setTimeout(() => {
      setTimeout(() => {
        // Rücksprung nach unten, zurückdrehen:
        wrapper.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
        wrapper.style.transform  = 'translateY(0) rotate(0deg)';
        img.src = this.images.mouthOpen || this.images.default;
        setTimeout(() => {
          if (!this._pettingActive) {
            this.img.src = this.images.default;
          }
          setTimeout(() => {
            wrapper.style.transition = origTransition || '';
            wrapper.style.transform = origTransform || '';
            if (typeof onFinish === 'function') onFinish();
          }, 700);
        }, 2000);
      }, 2200);
    }, 400);
  }

  _spawnHearts(count = 10) {
    for (let i = 0; i < count; ++i) {
      setTimeout(() => this._makeHeart(), Math.random() * 300);
    }
  }

  _makeHeart() {
    // Emoji oder eigenes Bild
    const emoji = Math.random() < 0.7 ? '❤️' : '💕';

    const heart = document.createElement('span');
    heart.textContent = emoji;
    heart.style.position = 'absolute';
    heart.style.left  = '30%';
    heart.style.bottom= '45%';
    heart.style.fontSize = `${16 + Math.random() * 14}px`;
    heart.style.pointerEvents = 'none';
    heart.style.opacity = '0.9';
    heart.style.zIndex = 10;

    // Start/End-Pos, Flugwinkel
    const angle = (Math.random() * Math.PI) - (Math.PI/2); // spread -90° to 90°
    const distance = 60 + Math.random() * 45;
    const dx = Math.cos(angle) * distance;
    const dy = Math.sin(angle) * distance;

    heart.animate([
      {
        transform: 'translate(-50%, 0) scale(1)',
        opacity: 0.95
      },
      {
        transform: `translate(calc(-50% + ${dx}px), ${-dy}px) scale(${1.3 + Math.random()*0.4}) rotate(${Math.random()*60-30}deg)`,
        opacity: 0.3
      }
    ], {
      duration: 1100 + Math.random()*800,
      easing: 'cubic-bezier(.28,1.01,.57,.99)'
    });

    // Remove after animation
    setTimeout(() => heart.remove(), 1600);

    // Im Cat-Wrapper anbringen:
    this.wrapper.appendChild(heart);
  }

  /** Call when streaming text begins */
  beginSpeech() {
    clearTimeout(this._speechTimeout);
    clearInterval(this._talkIntervalId);

    this._isSpeaking  = true;
    this._mouthOpen   = false;
    this.img.src      = this.images.default;
    this.bubble.style.opacity = '1';
    this.bubble.style.visibility = 'visible';
    // Bubble-Inhalt zurücksetzen (ohne den Pointer zu löschen!)
    Array.from(this.bubble.childNodes).forEach(node => {
      if (node !== this.bubblePointer) node.remove();
    });
    // Neuen (leeren) Textnode einfügen:
    this._bubbleTextNode = document.createTextNode('');
    this.bubble.appendChild(this._bubbleTextNode);

    this._talkIntervalId = setInterval(() => {
      this._mouthOpen = !this._mouthOpen;
      this.img.src = this._mouthOpen
        ? this.images.mouthOpen
        : this.images.default;
    }, this.talkInterval / 2);
  }

  /** Append a chunk of streamed text */
  appendSpeech(chunk) {
    if (this._bubbleTextNode)
      this._bubbleTextNode.textContent += chunk;
  }

  /** Call when the stream ends */
  endSpeech() {
    clearInterval(this._talkIntervalId);
    if (!this._pettingActive) {
      this.img.src = this.images.default;
    }
    this._speechTimeout = setTimeout(() => {
      this.bubble.style.opacity = '0';
      this._isSpeaking = false;
    }, 6000);
  }

  /** Clean up timers & DOM */
  destroy() {
    clearTimeout(this._blinkTimeout);
    clearInterval(this._talkIntervalId);
    clearTimeout(this._speechTimeout);
    this.wrapper.remove();
  }
}
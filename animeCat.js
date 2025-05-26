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
      joy:         'joy.png',
      mischievous: 'mischievous.png'
    }, options.images);
    this.blinkMin      = options.blinkMin      ?? 5000;
    this.blinkMax      = options.blinkMax      ?? 15000;
    this.blinkDuration = options.blinkDuration ?? 175;
    this.talkInterval  = options.talkInterval  ?? 300;
    this._consolationCount = 0;

    this._isSpeaking     = false;
    this._blinkTimeout   = null;
    this._talkIntervalId = null;
    this._speechTimeout  = null;
    this._mouthOpen      = false;
    this._pettingActive  = false;

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
      minWidth:      '180px',
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
    this.img.style.width = '62px';
    this.img.style.height = '50px';
    this.img.style.zIndex = '2';

    // --- Speech bubble ---
    this.bubble = document.createElement('div');
    Object.assign(this.bubble.style, {
      position:      'relative',
      marginLeft:    '14px',
      marginBottom:  '11px', 
      padding:       '10px 16px',
      background:    'white',
      border:        '1px solid #ccc',
      borderRadius:  '16px',
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

    // Emitter für Herzen (abgekoppelt!)
    this.heartEmitter = document.createElement('div');
    Object.assign(this.heartEmitter.style, {
      position: 'absolute',
      left: '82px',   // rechts neben der Katze
      bottom: '32px',
      pointerEvents: 'none',
      width: '1px',
      height: '1px',
      zIndex: 20
    });
    this.container.appendChild(this.heartEmitter);
  }

  // Bubble-Position absolut anpassen, wenn detached
  _positionBubbleDetached() {
    // Katze relativ im Container finden
    const catRect = this.img.getBoundingClientRect();
    const contRect = this.container.getBoundingClientRect();
    // "Andockpunkt": rechts neben der Katze, leicht versetzt nach oben
    const left = (catRect.right - contRect.left) + 12;
    const bottom = (contRect.bottom - catRect.bottom) + 8;
    Object.assign(this.bubble.style, {
      position: 'absolute',
      left: `${left}px`,
      bottom: `${bottom}px`,
      marginLeft: '0',
      marginBottom: '0'
    });
    // Tail bleibt am linken Rand der Bubble!
    this.bubblePointer.style.left = '-13px';
    this.bubblePointer.style.bottom = '12px';
  }
  _resetBubbleAttach() {
    Object.assign(this.bubble.style, {
      position: 'relative',
      left: '',
      bottom: '',
      marginLeft: '14px',
      marginBottom: '11px'
    });
    this.bubblePointer.style.left = '-13px';
    this.bubblePointer.style.bottom = '12px';
  }

  _startBlinking() {
    const delay = this.blinkMin + Math.random() * (this.blinkMax - this.blinkMin);
    this._blinkTimeout = setTimeout(() => {
      if (!this._isSpeaking) {
        this.img.src = this.images.blink;
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
    let joyActive = false;
    let mouseDown = false;
    let mouseDownAt = null;
    let lastPos = null;
    let moveDist = 0;

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
          this._consolation(mouseDownAt);
          mouseDown = false;
        }
      }, 4000);

      function onUp() {
        if (!mouseDown) return;
        cleanup();
        const heldFor = Date.now() - mouseDownAt;
        if (heldFor >= 4000) return; // already handled by timer above
        if (heldFor >= 4000 && moveDist >= MOVE_THRESHOLD) {
          joyActive = true;
          this._runJoyAnimation(() => {
            joyActive = false;
            reopenEyes();
            this._startBlinking();
          });
        } else if (heldFor > 1000) {
          this._consolation(mouseDownAt);
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
        this._pettingActive = false;
      };
      window.addEventListener('mousemove', onMoveBound);
      window.addEventListener('mouseup', onUpBound);
    });
  }

  _consolation(mouseDownAt) {
    // Trostpreis: Augen auf, dann mouth_open oder mischievous
    this._pettingActive = false;
    this.img.src = this.images.default;
    const heldFor = Date.now() - mouseDownAt;
    if (heldFor > 1000) {
      const mouthOpenTime = Math.max(heldFor - 1000, 1000);
      let imgToShow = this.images.mouthOpen || this.images.default;
      if (this._consolationCount >= 3) {
        if (
          this._consolationCount === 3 ||
          Math.random() < 0.2
        ) {
          imgToShow = this.images.mischievous || imgToShow;
        }
      }
      this.img.src = imgToShow;
      this._consolationCount++;
      this._startBlinking();
      setTimeout(() => {
        if (!this._isSpeaking && !this._pettingActive) {
          this.img.src = this.images.default;
        }
      }, mouthOpenTime);
    } else {
      this._startBlinking();
    }
  }

  _runJoyAnimation(onFinish) {
    const wrapper = this.wrapper;
    const img     = this.img;
    const origTransition = wrapper.style.transition;
    const origTransform  = wrapper.style.transform;

    img.src = this.images.joy || this.images.default;

    // 20% Chance auf Salto!
    const salto = Math.random() < 0.2;

    // Bubble abkoppeln wie gehabt
    this.container.appendChild(this.bubble);
    this._positionBubbleDetached();

    // Berechne Herz-Emitter-Position JETZT
    const catRect = this.img.getBoundingClientRect();
    const contRect = this.container.getBoundingClientRect();
    // Hier kannst du x/y Werte feinjustieren:
    this.heartEmitter.style.left = (catRect.right - contRect.left + 10) + 'px';
    this.heartEmitter.style.bottom = (contRect.bottom - catRect.bottom + 13) + 'px';

    const spawnHearts = () => this._spawnHearts(12);

    if (salto) {
      // 1. Hochspringen & direkt losdrehen
      wrapper.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
      wrapper.style.transform  = 'translateY(-40px) rotate(0deg)';
      setTimeout(() => {
        // 2. Während Sprung: Salto (Herzen werden genau jetzt “ausgestoßen”)
        wrapper.style.transition = 'transform 0.6s cubic-bezier(.19,1,.22,1)';
        wrapper.style.transform  = 'translateY(-40px) rotate(360deg)';
        spawnHearts();
        setTimeout(() => {
          // 3. Runterfallen (nur nach unten, KEIN weiteres Rotieren!)
          wrapper.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
          wrapper.style.transform  = 'translateY(0) rotate(360deg)';
          img.src = this.images.mouthOpen || this.images.default;
          setTimeout(() => {
            if (!this._pettingActive) this.img.src = this.images.default;
            setTimeout(() => {
              wrapper.style.transition = origTransition || '';
              wrapper.style.transform = origTransform || '';
              // Bubble zurück
              this.wrapper.appendChild(this.bubble);
              this._resetBubbleAttach();
              if (typeof onFinish === 'function') onFinish();
            }, 700);
          }, 2000);
        }, 600);
      }, 400);
    } else {
      // Normale Joy-Animation
      wrapper.style.transition = 'transform 0.6s cubic-bezier(.19,1,.22,1)';
      wrapper.style.transform  = 'translateY(-40px) rotate(12deg)';
      setTimeout(() => {
        spawnHearts();
        setTimeout(() => {
          wrapper.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
          wrapper.style.transform  = 'translateY(0) rotate(0deg)';
          img.src = this.images.mouthOpen || this.images.default;
          setTimeout(() => {
            if (!this._pettingActive) this.img.src = this.images.default;
            setTimeout(() => {
              wrapper.style.transition = origTransition || '';
              wrapper.style.transform = origTransform || '';
              // Bubble zurück
              this.wrapper.appendChild(this.bubble);
              this._resetBubbleAttach();
              if (typeof onFinish === 'function') onFinish();
            }, 700);
          }, 2000);
        }, 2200);
      }, 400);
    }
  }

  _spawnHearts(count = 10) {
    for (let i = 0; i < count; ++i) {
      setTimeout(() => this._makeHeart(), Math.random() * 300);
    }
  }

  _makeHeart() {
    const emoji = Math.random() < 0.7 ? '❤️' : '💕';

    const heart = document.createElement('span');
    heart.textContent = emoji;
    heart.style.position = 'absolute';
    heart.style.left  = '0%';
    heart.style.bottom= '0%';
    heart.style.fontSize = `${16 + Math.random() * 14}px`;
    heart.style.pointerEvents = 'none';
    heart.style.opacity = '0.9';
    heart.style.zIndex = 100;

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

    // Im Herzen-Emitter platzieren!
    this.heartEmitter.appendChild(heart);
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
    Array.from(this.bubble.childNodes).forEach(node => {
      if (node !== this.bubblePointer) node.remove();
    });
    this._bubbleTextNode = document.createTextNode('');
    this.bubble.appendChild(this._bubbleTextNode);

    this._talkIntervalId = setInterval(() => {
      this._mouthOpen = !this._mouthOpen;
      this.img.src = this._mouthOpen
        ? this.images.mouthOpen
        : this.images.default;
    }, this.talkInterval / 2);
  }

  appendSpeech(chunk) {
    if (this._bubbleTextNode)
      this._bubbleTextNode.textContent += chunk;
  }

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

  destroy() {
    clearTimeout(this._blinkTimeout);
    clearInterval(this._talkIntervalId);
    clearTimeout(this._speechTimeout);
    this.wrapper.remove();
    this.heartEmitter.remove();
  }
}
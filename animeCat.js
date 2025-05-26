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
    this.img.style.height = '60px';
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
          this.img.src = this.images.default;
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

    const closeEyes = () => {
      clearTimeout(holdTimer);
      this.img.src = this.images.eyesClosed;
    };
    const reopenEyes = () => {
      clearTimeout(holdTimer);
      if (joyActive) return; // don't override joy mode!
      if (!this._isSpeaking) {
        this.img.src = this.images.default;
      }
    };

    this.img.addEventListener('mousedown', () => {
      // if currently talking or animating, ignore hold-to-close
      if (this._isSpeaking || joyActive) return;
      closeEyes();
      // Nach 4s -> Joy-Animation starten!
      holdTimer = setTimeout(() => {
        joyActive = true;
        this._runJoyAnimation(() => {
          joyActive = false;
          reopenEyes();
        });
      }, 4000);
    });

    ['mouseup', 'mouseleave'].forEach(evt =>
      this.img.addEventListener(evt, () => {
        clearTimeout(holdTimer);
      })
    );
  }

    _runJoyAnimation(onFinish) {
    // Original-Pos und Style merken:
    const wrapper = this.wrapper;
    const img     = this.img;
    const origTransition = wrapper.style.transition;
    const origTransform  = wrapper.style.transform;

    // Bild auf joy.png setzen:
    img.src = this.images.joy || this.images.default;

    // Wrapper animieren: nach oben + rechts, leicht rotieren
    wrapper.style.transition = 'transform 0.6s cubic-bezier(.19,1,.22,1)';
    wrapper.style.transform  = 'translateY(-40px) rotate(12deg)';

    // Herzchen-Explosion starten
    this._spawnHearts(12); // oder mehr/weniger

    // Nach 0.4s (Sprung oben), dann warten (stehen lassen)
    setTimeout(() => {
      // Nach weiteren 2.6s zurückfallen (gesamt ca. 3s joy-mode)
      setTimeout(() => {
        // Rücksprung: nach unten
        wrapper.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
        wrapper.style.transform  = 'translateY(0) rotate(0deg)';
        // Bild zurück
        img.src = this.images.default;
        // Reset nach 0.5s
        setTimeout(() => {
          wrapper.style.transition = origTransition || '';
          wrapper.style.transform = origTransform || '';
          if (typeof onFinish === 'function') onFinish();
        }, 700);
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
    heart.style.left  = '35%';
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
    this.img.src = this.images.default;
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
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
      mouthOpen:   'mouth_open.png'
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
      padding:       '10px 16px',
      background:    'white',
      border:        '1px solid #ccc',
      borderRadius:  '16px 16px 16px 0',
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
    const closeEyes = () => {
      clearTimeout(holdTimer);
      this.img.src = this.images.eyesClosed;
    };
    const reopenEyes = () => {
      clearTimeout(holdTimer);
      if (!this._isSpeaking) {
        this.img.src = this.images.default;
      }
    };

    this.img.addEventListener('mousedown', () => {
      if (this._isSpeaking) return;
      closeEyes();
      holdTimer = setTimeout(reopenEyes, 4000);
    });
    ['mouseup', 'mouseleave'].forEach(evt =>
      this.img.addEventListener(evt, reopenEyes)
    );
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
    this.bubblePointer.style.display = '';
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
      this.bubblePointer.style.display = 'none';
      this._isSpeaking = false;
    }, 3000);
  }

  /** Clean up timers & DOM */
  destroy() {
    clearTimeout(this._blinkTimeout);
    clearInterval(this._talkIntervalId);
    clearTimeout(this._speechTimeout);
    this.wrapper.remove();
  }
}
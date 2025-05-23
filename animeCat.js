// animeCat.js
export class AnimeCat {
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
    this.blinkMin      = options.blinkMin     ?? 5000;
    this.blinkMax      = options.blinkMax     ?? 15000;
    this.blinkDuration = options.blinkDuration?? 175;
    this.talkInterval  = options.talkInterval ?? 300;

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
    this.wrapper = document.createElement('div');
    this.wrapper.style.position = 'relative';
    this.wrapper.style.display  = 'inline-block';

    // cat image
    this.img = document.createElement('img');
    this.img.src = this.images.default;
    // disable drag & selection
    this.img.draggable = false;
    this.img.style.userSelect       = 'none';
    this.img.style.webkitUserSelect = 'none';
    this.img.style.MozUserSelect    = 'none';
    this.img.style.msUserSelect     = 'none';
    // some browsers need this to stop the default drag ghost
    this.img.style.webkitUserDrag   = 'none';

    this.wrapper.appendChild(this.img);

    // speech bubble
    this.bubble = document.createElement('div');
    Object.assign(this.bubble.style, {
      position:      'absolute',
      bottom:        '100%',
      left:          '50%',
      transform:     'translateX(-50%)',
      padding:       '8px 12px',
      background:    'white',
      border:        '1px solid #ccc',
      borderRadius:  '4px',
      boxShadow:     '0 2px 6px rgba(0,0,0,0.2)',
      opacity:       '0',
      transition:    'opacity 0.3s',
      maxWidth:      '200px',
      wordWrap:      'break-word',
      fontFamily:    'sans-serif',
      fontSize:      '14px',
      color:         '#333',
      pointerEvents: 'none'
    });
    this.wrapper.appendChild(this.bubble);

    this.container.appendChild(this.wrapper);
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
      // if currently talking, ignore hold-to-close
      if (this._isSpeaking) return;
      closeEyes();
      // force reopen after max 5s
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
    this.bubble.textContent   = '';

    this._talkIntervalId = setInterval(() => {
      this._mouthOpen = !this._mouthOpen;
      this.img.src = this._mouthOpen
        ? this.images.mouthOpen
        : this.images.default;
    }, this.talkInterval / 2);
  }

  /** Append a chunk of streamed text */
  appendSpeech(chunk) {
    this.bubble.textContent += chunk;
  }

  /** Call when the stream ends */
  endSpeech() {
    clearInterval(this._talkIntervalId);
    this.img.src = this.images.default;
    this._speechTimeout = setTimeout(() => {
      this.bubble.style.opacity = '0';
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
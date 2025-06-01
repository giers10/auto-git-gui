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
    this._startGlowWobble();
  }

  _createElements() {
    // Outer Wrapper für Flex-Layout
    this.wrapper = document.createElement('div');
    Object.assign(this.wrapper.style, {
      position:      'relative',
      display:       'flex',
      flexDirection: 'row',
      alignItems:    'flex-end',
      minHeight:     '80px',
      minWidth:      '180px',
      zIndex:        1
    });

    // Innerer Container für Glow + Katze (Stack!)
    this.catContainer = document.createElement('div');
    Object.assign(this.catContainer.style, {
      position:      'relative',
      width:         '90px',
      height:        '70px',
      minWidth:      '90px',
      minHeight:     '70px',
      zIndex:        2
    });

    // Glow (hinter der Katze)
    this.glow = document.createElement('div');
    this.glow.id = 'cat-glow';
    Object.assign(this.glow.style, {
      position:        'absolute',
      left:            '40%',
      top:             '73%',
      transform:       'translate(-50%, -50%)',
      borderRadius:    '50%',
      width:           '70px',      // ODER: Teste 54, 70, 80 – wie es dir am besten gefällt!
      height:          '52px',
      pointerEvents:   'none',
      zIndex:          1,
      filter:          'blur(4px)',    // ruhig auch 14px für weichen Rand!
      opacity:         '0.85',           // Wieviel du willst!
      transition:      'background 0.4s, opacity 0.3s'
    });
    this.catContainer.appendChild(this.glow);

    // Cat image (über dem Glow, mittig im catContainer)
    this.img = document.createElement('img');
    this.img.src = this.images.default;
    Object.assign(this.img.style, {
      position:       'absolute',
      left:           '40%',
      top:            '66%',
      transform:      'translate(-50%, -50%)',
      width:          '62px',
      height:         '50px',
      zIndex:         2,
      userSelect:     'none',
      webkitUserSelect: 'none',
      MozUserSelect:     'none',
      msUserSelect:      'none',
      webkitUserDrag:    'none',
      pointerEvents:  'auto'
    });
    this.img.draggable = false;
    this.catContainer.appendChild(this.img);

    // Sprechblase (direkt im wrapper)
    this.bubble = document.createElement('div');
    Object.assign(this.bubble.style, {
      position:      'relative',
      marginLeft:    '4px',     // vorher 14px
      marginBottom:  '1px',     // vorher 11px
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
      zIndex:        10,
      overflowWrap:  'anywhere',
      display:       'flex',
      alignItems:    'center'
    });

    // Bubble Tail
    this.bubblePointer = document.createElement('div');
    Object.assign(this.bubblePointer.style, {
      position:      'absolute',
      left:          '-13px',
      bottom:        '14px',
      width:         '0',
      height:        '0',
      borderTop:     '8px solid transparent',
      borderBottom:  '8px solid transparent',
      borderRight:   '13px solid #fff',
      filter:        'drop-shadow(-2px 1px 2px rgba(0,0,0,0.10))',
      zIndex:        11
    });
    this.bubble.appendChild(this.bubblePointer);

    // Zusammenbauen
    this.wrapper.appendChild(this.catContainer); // LINKS (Stack: Glow, Katze)
    this.wrapper.appendChild(this.bubble);       // RECHTS
    this.container.appendChild(this.wrapper);

    // Herz-Emitter, bleibt wie gehabt
    this.heartEmitter = document.createElement('div');
    Object.assign(this.heartEmitter.style, {
      position: 'absolute',
      left:     '90px',
      bottom:   '50px',
      pointerEvents: 'none',
      width:    '1px',
      height:   '1px',
      zIndex:   20
    });
    this.container.appendChild(this.heartEmitter);
  }


  _startGlowWobble() {
    let t = 0;
    const loop = () => {
      // Sanfte Wellen-Animation der Opazität
      const phase = 0.92 + 0.1 * Math.sin(t);
      this.glow.style.opacity = phase.toFixed(2);
      // Wenn du magst: this.glow.style.filter = `blur(${10 + 2*Math.sin(t/2)}px)`;
      t += 0.03;
      this._glowWobbleId = requestAnimationFrame(loop);
    };
    loop();
  }
  stopGlowWobble() {
    if (this._glowWobbleId) cancelAnimationFrame(this._glowWobbleId);
  }

// Hilfsfunktionen zur Farbinterpolation
_lerp(a, b, t) { return a + (b - a) * t; }
_lerpColor(a, b, t) {
  return [
    Math.round(this._lerp(a[0], b[0], t)),
    Math.round(this._lerp(a[1], b[1], t)),
    Math.round(this._lerp(a[2], b[2], t)),
  ];
}

// Animiert den Glow je nach Commit-Anzahl
  animateCatGlow(commitCount) {
    const glow = this.glow;
    if (!glow) return;

    // Farbverlauf – nur Farbe wechseln!
    const stops = [
      { c:   0, color: [157, 157, 157] },
      { c:   5, color: [255, 255, 255] },
      { c:  15, color: [30, 255, 0] },
      { c:  50, color: [0, 112, 221] },
      { c: 100, color: [163, 53, 238] },
      { c: 500, color: [255, 128, 0] }
    ];

    let lower = stops[0], upper = stops[stops.length-1];
    for (let i=0; i<stops.length-1; ++i) {
      if (commitCount >= stops[i].c && commitCount < stops[i+1].c) {
        lower = stops[i]; upper = stops[i+1]; break;
      }
    }
    const range = upper.c - lower.c || 1;
    const t = Math.min(Math.max((commitCount - lower.c) / range, 0), 1);

    const [r,g,b] = this._lerpColor(lower.color, upper.color, t);
    this.glow.style.background = `radial-gradient(
      ellipse at 50% 50%,
      rgba(${r},${g},${b},0.97) 0%,
      rgba(${r},${g},${b},0.24) 70%,
      rgba(0,0,0,0) 100%
    )`;
  }

  // Bubble für Animation absolut an Position der Katze anheften
  _positionBubbleDetached() {
    // Katzen-Bild in catContainer suchen!
    const catRect = this.catContainer.getBoundingClientRect();
    const contRect = this.container.getBoundingClientRect();

    // Position: leicht rechts, minimal nach oben (bei Bedarf feintunen)
    const left   = (catRect.right - contRect.left) + 2;   // vorher +12, jetzt +2
    const bottom = (contRect.bottom - catRect.bottom) - 2; // vorher +8, jetzt -2
    Object.assign(this.bubble.style, {
      position:   'absolute',
      left:       `${left}px`,
      bottom:     `${bottom}px`,
      marginLeft: '0',
      marginBottom: '0'
    });
    // Tail am linken Rand
    this.bubblePointer.style.left = '-13px';
    this.bubblePointer.style.bottom = '14px';
  }

  // Zurück zum Flex-Layout
  _resetBubbleAttach() {
    Object.assign(this.bubble.style, {
      position:    'relative',
      left:        '',
      bottom:      '',
      marginLeft:  '12px',
      marginBottom:'16px'
    });
    this.bubblePointer.style.left = '-13px';
    this.bubblePointer.style.bottom = '14px';
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
    const MOVE_THRESHOLD = 200;

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
  const img = this.img;
  const catBox = this.catContainer;
  const origCatTransition = catBox.style.transition;
  const origCatTransform  = catBox.style.transform;

  img.src = this.images.joy || this.images.default;

  // 20% Chance auf Salto!
  const salto = Math.random() < 0.2;

  // Bubble abkoppeln
  this.container.appendChild(this.bubble);
  this._positionBubbleDetached();

  // Herz-Emitter anpassen
  const catRect = this.catContainer.getBoundingClientRect();
  const contRect = this.container.getBoundingClientRect();
  const emitterX = catRect.right - contRect.left + 12;
  const emitterY = contRect.bottom - catRect.bottom + 34;
  this.heartEmitter.style.left = emitterX + 'px';
  this.heartEmitter.style.bottom = emitterY + 'px';

  const spawnHearts = () => this._spawnHearts(12);

  if (salto) {
    // Salto: gesamter Container springt & dreht
    catBox.style.transition = 'transform 1.2s cubic-bezier(.19,1,.22,1)';
    catBox.style.transform  = 'translateY(-40px) rotate(360deg)';
    spawnHearts();

    setTimeout(() => {
      catBox.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
      catBox.style.transform  = 'translateY(0) rotate(360deg)';
      img.src = this.images.mouthOpen || this.images.default;
      setTimeout(() => {
        if (!this._pettingActive) img.src = this.images.default;
        setTimeout(() => {
          catBox.style.transition = origCatTransition || '';
          catBox.style.transform = origCatTransform || '';
          this.wrapper.appendChild(this.bubble);
          this._resetBubbleAttach();
          if (typeof onFinish === 'function') onFinish();
        }, 700);
      }, 2000);
    }, 1200);
  } else {
    // Normaler Joy-Jump
    catBox.style.transition = 'transform 0.6s cubic-bezier(.19,1,.22,1)';
    catBox.style.transform  = 'translateY(-40px) rotate(12deg)';
    setTimeout(() => {
      spawnHearts();
      setTimeout(() => {
        catBox.style.transition = 'transform 0.8s cubic-bezier(.19,1,.22,1)';
        catBox.style.transform  = 'translateY(0) rotate(0deg)';
        img.src = this.images.mouthOpen || this.images.default;
        setTimeout(() => {
          if (!this._pettingActive) img.src = this.images.default;
          setTimeout(() => {
            catBox.style.transition = origCatTransition || '';
            catBox.style.transform = origCatTransform || '';
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
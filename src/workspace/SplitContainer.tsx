/**
 * Rendering the pane tree.
 *
 * A split renders its two children with a draggable divider between them. The
 * drag updates the ratio in the layout, which is persisted with the workspace,
 * so a split survives a restart at the size the user left it.
 */

import type { ReactNode } from 'react';
import { useCallback, useRef } from 'react';

import type { PaneNode } from '@/types/domain';

export interface SplitContainerProps {
  node: PaneNode;
  renderLeaf: (leaf: Extract<PaneNode, { type: 'leaf' }>) => ReactNode;
  onResize: (splitId: string, ratio: number) => void;
}

export function SplitContainer({ node, renderLeaf, onResize }: SplitContainerProps) {
  if (node.type === 'leaf') {
    return <>{renderLeaf(node)}</>;
  }

  return (
    <div className={`ie-split ie-split--${node.direction}`} data-split-id={node.id}>
      <div className="ie-split__side" style={{ flexBasis: `${node.ratio * 100}%` }}>
        <SplitContainer node={node.first} renderLeaf={renderLeaf} onResize={onResize} />
      </div>
      <Divider
        direction={node.direction}
        ratio={node.ratio}
        onResize={(ratio) => onResize(node.id, ratio)}
      />
      <div className="ie-split__side" style={{ flexBasis: `${(1 - node.ratio) * 100}%` }}>
        <SplitContainer node={node.second} renderLeaf={renderLeaf} onResize={onResize} />
      </div>
    </div>
  );
}

interface DividerProps {
  direction: 'horizontal' | 'vertical';
  ratio: number;
  onResize: (ratio: number) => void;
}

function Divider({ direction, ratio, onResize }: DividerProps) {
  const element = useRef<HTMLDivElement | null>(null);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const container = element.current?.parentElement;
      if (!container) return;

      event.preventDefault();
      // Pointer capture keeps the drag alive even when the pointer leaves the
      // divider, which it immediately does.
      event.currentTarget.setPointerCapture(event.pointerId);

      const rect = container.getBoundingClientRect();
      const move = (moveEvent: PointerEvent) => {
        const fraction =
          direction === 'vertical'
            ? (moveEvent.clientX - rect.left) / rect.width
            : (moveEvent.clientY - rect.top) / rect.height;
        onResize(fraction);
      };
      const up = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    [direction, onResize],
  );

  return (
    <div
      ref={element}
      className={`ie-split__divider ie-split__divider--${direction}`}
      role="separator"
      aria-orientation={direction === 'vertical' ? 'vertical' : 'horizontal'}
      aria-valuenow={Math.round(ratio * 100)}
      aria-valuemin={15}
      aria-valuemax={85}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={(event) => {
        // Keyboard resizing, so a split is adjustable without a pointer.
        const step = event.shiftKey ? 0.1 : 0.02;
        if (
          (direction === 'vertical' && event.key === 'ArrowLeft') ||
          (direction === 'horizontal' && event.key === 'ArrowUp')
        ) {
          event.preventDefault();
          onResize(ratio - step);
        }
        if (
          (direction === 'vertical' && event.key === 'ArrowRight') ||
          (direction === 'horizontal' && event.key === 'ArrowDown')
        ) {
          event.preventDefault();
          onResize(ratio + step);
        }
      }}
    />
  );
}

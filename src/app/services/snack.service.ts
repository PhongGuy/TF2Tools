import { Injectable, OnDestroy } from '@angular/core';
import { MatSnackBar, MatSnackBarConfig, MatSnackBarRef, SimpleSnackBar } from '@angular/material/snack-bar';
import { Subscription } from 'rxjs';

/**
 * Snack bar message
 */
export class SnackBarMessage {
  /**
   * Message  of snack bar message
   */
  message: string;
  /**
   * Action  of snack bar message
   */
  action: string = null;
  /**
   * Config  of snack bar message
   */
  config: MatSnackBarConfig = null;
}

/**
 * Snack service
 */
@Injectable({
  providedIn: 'root'
})
export class SnackService implements OnDestroy {

  /**
   * Message queue of snack service
   */
  private messageQueue: Array<any> = Array<any>();
  /**
   * Subscription  of snack service
   */
  private subscription: Subscription;
  /**
   * Snack bar ref of snack service
   */
  private snackBarRef: MatSnackBarRef<SimpleSnackBar>;
  /**
   * Determines whether instance visible is
   */
  private isInstanceVisible = false;

  /**
   * Creates an instance of snack service.
   *
   * @param snack
   */
  constructor(
    private snack: MatSnackBar
  ) { }

  /**
   * on destroy
   */
  ngOnDestroy(): void  {
    this.subscription.unsubscribe();
  }

  /**
   * Shows snack service
   *
   * @param message The message to show in the snackbar.
   * @param [action] The label for the snackbar action.
   * @param [duration] Additional configuration options for the snackbar.
   * @param [classOverride] Adds a css class on the snackbar so you can add color.
   */
  show(
    message: string,
    action?: string,
    duration?: number,
    classOverride: string = 'blue-snackbar'
  ): void {
    const config = new MatSnackBarConfig();
    config.duration = (duration ? duration : 3000);
    config.verticalPosition = 'bottom';
    config.horizontalPosition = 'left';
    config.panelClass = [classOverride];

    const sbMessage = new SnackBarMessage();
    sbMessage.message = message;
    sbMessage.action = action;
    sbMessage.config = config;

    this.messageQueue.push(sbMessage);

    if (!this.isInstanceVisible) {
      this.showNext();
    }
  }

  /**
   * Shows next
   *
   * @returns
   */
  private showNext() {
    if (this.messageQueue.length === 0) {
      return;
    }

    const message = this.messageQueue.shift();
    this.isInstanceVisible = true;

    this.snackBarRef = this.snack.open(
      message.message,
      message.action,
      message.config
    );

    this.snackBarRef.afterDismissed().subscribe(() => {
      this.isInstanceVisible = false;
      this.showNext();
    });
  }
}

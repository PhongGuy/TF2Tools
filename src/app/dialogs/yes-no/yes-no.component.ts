import { Component, Inject } from '@angular/core';
import { MAT_DIALOG_DATA } from '@angular/material/dialog';
import { YesNo } from '../../models/yesNo';

/**
 * Yes No
 */
@Component({
  selector: 'app-yes-no',
  templateUrl: './yes-no.component.html'
})
export class YesNoComponent {

  /**
   * Creates an instance of yes no component.
   *
   * @param data YesNo
   */
  constructor(
    @Inject(MAT_DIALOG_DATA) public data: YesNo
  ) { }

}

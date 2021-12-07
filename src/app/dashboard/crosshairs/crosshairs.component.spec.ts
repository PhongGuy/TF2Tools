import { ComponentFixture, TestBed } from '@angular/core/testing';

import { CrosshairsComponent } from './crosshairs.component';

describe('CrosshairsComponent', () => {
  let component: CrosshairsComponent;
  let fixture: ComponentFixture<CrosshairsComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      declarations: [ CrosshairsComponent ]
    })
    .compileComponents();
  });

  beforeEach(() => {
    fixture = TestBed.createComponent(CrosshairsComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });
});

import React from 'react';
import { closeModal } from '../actions/modal_actions';
import { connect } from 'react-redux';

function Modal({ component, closeModal }) {
  
  if (!component.length) {
    return null;
  }

  const Component = component[0]
  
  return (
    <div className="modal-background" onClick={closeModal}>
      <div className="modal-child" onClick={e => e.stopPropagation()}>
        <Component />
      </div>
    </div>

  );
}


const mapStateToProps = state => {
  return {
    component: state.ui.modal.component
  };
};

const mapDispatchToProps = dispatch => {
  return {
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(Modal);
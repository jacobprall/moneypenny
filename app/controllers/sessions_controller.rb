class SessionsController < ApplicationController
  def new
    @user = User.new
  end

  def create
    @user = User.find_by_credentials(params[:user][:email], params[:user][:password])
    if @user 
      login!(@user)
    else 
      flash[:errors] = ['Invalid Username or Password']
    end
  end

  def destroy
    logout!
  end

end
